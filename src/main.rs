//use anyhow::Ok;
use k8s_openapi::api::core::v1::ContainerState;
use std::collections::HashSet;
//use tracing::*;
use crate::crd::PodMonitor;
use apiexts::CustomResourceDefinition;
use futures::stream::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1 as apiexts;
use kube::runtime::watcher::Config;
use kube::Resource;
use kube::ResourceExt;
use kube::{
    api::{Api, ListParams, Patch, PatchParams},
    client::Client,
    runtime::controller::Action,
    runtime::wait::{await_condition, conditions},
    runtime::Controller,
    CustomResourceExt,
};
use log::debug;
use log::error;
use log::info;
// use log::warn;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::Duration;

pub mod crd;
mod finalizer;
mod utils;

use std::sync::{Mutex, OnceLock};

fn pod_state() -> &'static Mutex<HashMap<String, Mutex<HashMap<String, String>>>> {
    static HASHMP: OnceLock<Mutex<HashMap<String, Mutex<HashMap<String, String>>>>> =
        OnceLock::new();
    HASHMP.get_or_init(|| Mutex::new(HashMap::new()))
}

#[tokio::main]
async fn main() -> () {
    env_logger::init();
    info!("Starting...");
    let kubernetes_client: Client = Client::try_default()
        .await
        .expect("Kindly set KUBECONFIG environment variable.");
    let ssapply = PatchParams::apply("podmonitor_apply").force();
    let crds: Api<CustomResourceDefinition> = Api::all(kubernetes_client.clone());
    let _ = crds
        .patch(
            "podmonitors.kk.dev",
            &ssapply,
            &Patch::Apply(PodMonitor::crd()),
        )
        .await;
    info!("Crd Submitted to the API-Server...");
    let establish = await_condition(crds, "podmonitor.kk.dev", conditions::is_crd_established());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), establish).await;

    let crd_api: Api<PodMonitor> = Api::all(kubernetes_client.clone());
    let context: Arc<ContextData> = Arc::new(ContextData::new(kubernetes_client.clone()));

    Controller::new(crd_api.clone(), Config::default())
        .run(reconcile, on_error, context)
        .for_each(|reconciliation_result| async move {
            match reconciliation_result {
                Ok(podmonitor_resource) => {
                    info!(
                        "Monitoring Successful for Resource: {:?}, Namespace: {:?}",
                        podmonitor_resource.0.name, podmonitor_resource.0.namespace
                    );
                }
                Err(reconciliation_err) => {
                    error!("Monitoring  Error:\n{:?}", reconciliation_err);
                }
            }
        })
        .await;
    ()
}

async fn monitor_pods_in_namespace(
    pod_api: &Api<Pod>,
    namespace: &str,
    monitored_pods: &Vec<String>,
) -> Result<HashMap<String, (Vec<String>, Vec<String>, String)>, Error> {
    let mut error_pod: HashMap<String, (Vec<String>, Vec<String>, String)> = HashMap::new();
    let mut all_pod_names: HashSet<String> = HashSet::new();
    let mut pods_in_namespace: HashMap<String, String> = HashMap::new();
    debug!("Global Pod states {:?}", pod_state().lock().unwrap());
    pod_state()
        .lock()
        .unwrap()
        .entry(namespace.to_owned())
        .or_insert(pods_in_namespace.clone().into());
    for p in pod_api.list(&ListParams::default()).await? {
        let name: String = p.name_any();
        if monitored_pods.is_empty() || monitored_pods.contains(&name.to_string()) {
            all_pod_names.insert(name.clone());

            let pod_status = p
                .status
                .clone()
                .unwrap()
                .container_statuses
                .unwrap()
                .iter()
                .map(|c| c.state.clone())
                .collect::<Vec<Option<ContainerState>>>();

            let cont_status = pod_status
                .clone()
                .iter()
                .map(|cs| match cs.clone().unwrap().running {
                    Some(_r) => return "Running".to_string(),
                    None => return "Not Running".to_string(),
                })
                .collect::<Vec<String>>();

            let cont_reason = pod_status
                .clone()
                .iter()
                .map(|cr| match cr.clone().unwrap().waiting {
                    Some(_w) => return _w.reason.unwrap(),
                    None => return "None".to_string(),
                })
                .collect::<Vec<String>>();

            let phase = &p.status.unwrap().phase.unwrap();
            debug!(
                "Pod Name{:?}, Containers Statuses {:?}, Containers Reasions {:?}, Pod Phase {:?}",
                name, cont_status, cont_reason, phase
            );

            let c_reasons = cont_reason
                .clone()
                .into_iter()
                .filter(|i| !["Running", "Succeeded", "ContainerCreating"].contains(&&i[..]))
                .collect::<Vec<String>>();
            // debug!("{:?}{:?}", pod_state().lock().unwrap().get(&namespace.to_owned()).unwrap(), Some(&phase));
            if !c_reasons.is_empty()
                && &phase[..] != "Running"
                && pod_state()
                    .lock()
                    .unwrap()
                    .get(&namespace.to_owned())
                    .expect("Internal State Error")
                    .lock()
                    .unwrap()
                    .get(&name)
                    != Some(&phase)
            {
                error_pod.insert(
                    name.clone(),
                    (cont_status, cont_reason.clone(), phase.to_string()),
                );
            }
            if !&cont_reason.contains(&"ContainerCreating".to_string()) {
                pod_state()
                    .lock()
                    .unwrap()
                    .get(&namespace.to_owned())
                    .unwrap()
                    .lock()
                    .unwrap()
                    .insert(name.clone(), phase.clone());
            }
        }
    }

    let pod_names: Vec<String> = pod_state()
        .lock()
        .unwrap()
        .get(&namespace.to_owned())
        .expect("Internal State Error")
        .lock()
        .unwrap()
        .clone()
        .into_keys()
        .collect();
    let set_pod_names: HashSet<String> = HashSet::from_iter(pod_names);
    let deleted_pod: HashSet<&String> = set_pod_names.difference(&all_pod_names).collect();
    info!("Deleted Pod: {:?}", deleted_pod);

    for pod in deleted_pod {
        pod_state()
            .lock()
            .unwrap()
            .get(&namespace.to_owned())
            .expect("Internal State Error")
            .lock()
            .unwrap()
            .remove(pod);
    }

    Ok(error_pod)
}

async fn prepare_email(
    podmonitor: &Arc<PodMonitor>,
    error_pods: HashMap<String, (Vec<String>, Vec<String>, String)>,
) -> Result<(), Error> {
    info!("Setting Up Email...");
    debug!("{:?}", error_pods);
    let mut msg = String::new();
    msg.push_str(&"Hi,\n\nFollowing Pods are not in running state.\n\n");
    for (key, value) in error_pods.into_iter() {
        let c_status = value.0.join(",");
        let c_reason = value.1.join(",");
        let p_phase = value.2;
        let ns = &podmonitor.spec.target_namespace;
        msg.push_str(&format!("Namespace: {ns}\nPod Name : {key} \nContainers Statuses: {c_status} \nStatus Remark:  {c_reason} \nPOD_STATE: {p_phase}\n\n--------\n\n"));
    }
    msg.push_str(&"Thanks");
    debug!("{}", msg);
    utils::send_email(
        &podmonitor.name_any(),
        podmonitor
            .spec
            .get_mail_to()
            .map(|s| s.as_str())
            .unwrap_or("default_to@example.com"),
        podmonitor
            .spec
            .get_mail_from()
            .map(|s| s.as_str())
            .unwrap_or("default_from@example.com"),
        // &podmonitor.spec.mail_to,
        // &podmonitor.spec.mail_from,
        &msg,
        podmonitor
            .spec
            .get_smtp_server()
            .map(|s| s.as_str())
            .unwrap_or("127.0.0.1"),
        *podmonitor.spec.get_smtp_port().unwrap_or(&25),
        &podmonitor.spec.get_tls(),
        &podmonitor.spec.get_mail_username().as_deref(),
        &podmonitor.spec.get_mail_password().as_deref(),
    )
    .await;
    // info!("Email Sent!!!");
    Ok(())
}

async fn call_webhook(
    podmonitor: &Arc<PodMonitor>,
    error_pods: HashMap<String, (Vec<String>, Vec<String>, String)>,
) -> Result<(), Error> {
    let default_url: String = "http://example.com".to_string();
    let url: &String = &podmonitor.spec.get_webhook_url().unwrap_or(&default_url);
    let json_str = serde_json::to_string(&error_pods).unwrap_or("{}".to_string());
    let mut json_value: Value =
        serde_json::from_str(&json_str).unwrap_or(serde_json::Value::String("{}".to_string()));
    if let Some(obj) = json_value.as_object_mut() {
        obj.insert(
            "namespace".to_string(),
            json!(&podmonitor.spec.target_namespace),
        );
    }

    let data = serde_json::to_string_pretty(&json_value).unwrap_or("{}".to_string());

    match utils::post_data(&url, serde_json::Value::String(data)).await {
        Ok(response) => info!("Webhook request {}", response),
        Err(e) => error!("Error: {}", e),
    }
    Ok(())
}

struct ContextData {
    client: Client,
}

impl ContextData {
    pub fn new(client: Client) -> Self {
        ContextData { client }
    }
}

enum PodMonitorAction {
    Create,
    Delete,
    NoOp,
}

async fn reconcile(
    podmonitor: Arc<PodMonitor>,
    context: Arc<ContextData>,
) -> Result<Action, Error> {
    let client: Client = context.client.clone();
    let namespace: String = match podmonitor.namespace() {
        None => {
            return Err(Error::UserInputError(
                "Pod Monitor Resource is namespaced. Kindly provide namespace for resource"
                    .to_owned(),
            ));
        }
        Some(namespace) => namespace,
    };
    let name = podmonitor.name_any();

    match determine_action(&podmonitor) {
        PodMonitorAction::Create => {
            finalizer::add(client.clone(), &name, &namespace).await?;
            Ok(Action::requeue(Duration::from_secs(10)))
        }
        PodMonitorAction::Delete => {
            finalizer::delete(client, &name, &namespace).await?;
            Ok(Action::await_change())
        }
        PodMonitorAction::NoOp => {
            let client = Client::try_default().await?;
            let target_namespace = &podmonitor.spec.target_namespace;
            // let target_pods = &podmonitor.spec.target_pods;
            let pods: Api<Pod> = Api::namespaced(client, &target_namespace);

            let error_pods = match &podmonitor.spec.target_pods {
                Some(t_pods) => {
                    let error_pods =
                        monitor_pods_in_namespace(&pods, &target_namespace, &t_pods).await?;
                    error_pods
                }
                None => {
                    let not_pods: Vec<String> = Vec::new();
                    let error_pods =
                        monitor_pods_in_namespace(&pods, &target_namespace, &not_pods).await?;
                    error_pods
                }
            };

            if error_pods.is_empty() {
                return Ok(Action::requeue(Duration::from_secs(10)));
            }

            match &podmonitor.spec.mail {
                Some(mail) => prepare_email(&podmonitor, error_pods.clone()).await?,
                None => println!(""),
            }
            match &podmonitor.spec.webhook {
                Some(webhook) => call_webhook(&podmonitor, error_pods.clone()).await?,
                None => println!(""),
            }
            Ok(Action::requeue(Duration::from_secs(10)))
        }
    }
}

fn determine_action(podmonitor: &PodMonitor) -> PodMonitorAction {
    if podmonitor.meta().deletion_timestamp.is_some() {
        PodMonitorAction::Delete
    } else if podmonitor
        .meta()
        .finalizers
        .as_ref()
        .map_or(true, |finalizers| finalizers.is_empty())
    {
        PodMonitorAction::Create
    } else {
        PodMonitorAction::NoOp
    }
}

fn on_error(podmonitor: Arc<PodMonitor>, error: &Error, _context: Arc<ContextData>) -> Action {
    error!("Error While Monitoring:\n{:?}.\n{:?}", error, podmonitor);
    Action::requeue(Duration::from_secs(5))
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Kubernetes reported error: {source}")]
    KubeError {
        #[from]
        source: kube::Error,
    },

    #[error("Invalid podmonitor CRD: {0}")]
    UserInputError(String),
}
