//use anyhow::Ok;
use std::collections::HashSet;
use k8s_openapi::api::core::v1::ContainerState;
//use tracing::*;
use std::sync::Arc;
use std::collections::HashMap;
use futures::stream::StreamExt;
use kube::runtime::watcher::Config;
use kube::Resource;
use kube::ResourceExt;
use apiexts::CustomResourceDefinition;
use k8s_openapi::{apiextensions_apiserver::pkg::apis::apiextensions::v1 as apiexts};
use kube::{CustomResourceExt, client::Client, runtime::controller::Action, runtime::Controller, api::{Api, Patch, PatchParams, ListParams}, runtime::wait::{await_condition, conditions}};
use tokio::time::Duration;
use k8s_openapi::api::core::v1::Pod;
use crate::crd::PodMonitor;

pub mod crd;
mod finalizer;
mod utils;



use std::sync::{Mutex, OnceLock};

fn pod_state() -> &'static Mutex<HashMap<String, Mutex<HashMap<String, String>>>> {
    static HASHMP: OnceLock<Mutex<HashMap<String, Mutex<HashMap<String, String>>>>> = OnceLock::new();
    HASHMP.get_or_init(|| Mutex::new(HashMap::new()))
}


#[tokio::main]
async fn main() -> () {

    println!("Starting...");
    let kubernetes_client: Client = Client::try_default()
        .await
        .expect("Kindly set KUBECONFIG environment variable.");
    let ssapply = PatchParams::apply("podmonitor_apply").force();
    let crds: Api<CustomResourceDefinition> = Api::all(kubernetes_client.clone());
    let _ = crds.patch("podmonitors.kk.dev", &ssapply, &Patch::Apply(PodMonitor::crd())).await;
    println!("Crd Submitted to the API-Server...");
    let establish = await_condition(crds, "podmonitor.kk.dev", conditions::is_crd_established());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), establish).await;

    let crd_api: Api<PodMonitor> = Api::all(kubernetes_client.clone());
    let context: Arc<ContextData> = Arc::new(ContextData::new(kubernetes_client.clone()));

    Controller::new(crd_api.clone(), Config::default())
        .run(reconcile, on_error, context)
        .for_each(|reconciliation_result| async move {
            match reconciliation_result {
                Ok(podmonitor_resource) => {
                    println!("Monitoring Successful for Resource: {:?}, Namespace: {:?}", podmonitor_resource.0.name, podmonitor_resource.0.namespace);
                    
                }
                Err(reconciliation_err) => {
                    eprintln!("Monitoring  Error:\n{:?}", reconciliation_err);
                }
            }
        })
        .await;
()
}

async fn monitor_pods_in_namespace(pod_api: Api<Pod>, namespace: &str) -> Result<HashMap<String, (Vec<String>, Vec<String>, String)>, Error> {

    let mut error_pod: HashMap<String, (Vec<String>, Vec<String>, String)> = HashMap::new();
    
    let mut all_pod_names: HashSet<String> = HashSet::new();
    let mut pods_in_namespace: HashMap<String, String> = HashMap::new();
    println!("{:?}", pod_state().lock().unwrap());
    pod_state().lock().unwrap().entry(namespace.to_owned()).or_insert(pods_in_namespace.clone().into());
    for p in pod_api.list(&ListParams::default()).await? {

        let name: String = p.name_any();
        all_pod_names.insert(name.clone());
        let pod_status = p.status.clone().unwrap()
        .container_statuses.unwrap().iter()
        .map(|c| c.state.clone()).collect::<Vec<Option<ContainerState>>>();
        
        let cont_status = pod_status.clone().iter()
        .map(|cs| match cs.clone().unwrap().running { Some(_r) => return "Running".to_string(), None => return "Not Running".to_string() }).collect::<Vec<String>>();
        let cont_reason = pod_status.clone().iter()
        .map(|cr| match cr.clone().unwrap().waiting { Some(_w) => return _w.reason.unwrap(), None => return "None".to_string() }).collect::<Vec<String>>();
        let phase = &p.status.unwrap().phase.unwrap();
        println!("{:?}, {:?}, {:?}, {:?}", name, cont_status, cont_reason, phase);

        let c_reasons = cont_reason.clone().into_iter().filter(|i| !["Running", "Succeeded", "ContainerCreating"].contains(&&i[..])).collect::<Vec<String>>();
     //   println!("{:?}{:?}", pod_state().lock().unwrap().get(&namespace.to_owned()).unwrap(), Some(&phase));
        if !c_reasons.is_empty() && &phase[..] != "Running" && pod_state().lock().unwrap().get(&namespace.to_owned()).expect("Internal State Error").lock().unwrap().get(&name) != Some(&phase) {
      //  if !c_reasons.is_empty() && &phase[..] != "Running" && pod_state().lock().unwrap().get(&namespace.to_owned()).unwrap().lock().unwrap().get(&name) != Some(&phase) {
     //   println!("{:?}\n{:?}",  pod_state().lock().unwrap(), &phase);
       
        error_pod.insert(name.clone(), (cont_status, cont_reason.clone(), phase.to_string()));
       }  
        if !&cont_reason.contains(&"ContainerCreating".to_string()){
       // pods_in_namespace.insert(name.clone(), phase.clone());
               pod_state().lock().unwrap().get(&namespace.to_owned()).unwrap().lock().unwrap().insert(name.clone(), phase.clone());
       }
    }
   // pod_state().lock().unwrap().insert(namespace.to_owned(), pods_in_namespace.into());
    //let pod_names: Vec<String> = pod_state().lock().unwrap().get(&namespace.to_owned()).expect("Internal State Error").lock().unwrap().clone().into_keys().collect();
    let pod_names: Vec<String> = pod_state().lock().unwrap().get(&namespace.to_owned()).expect("Internal State Error").lock().unwrap().clone().into_keys().collect();
    //let pod_names: Vec<String> = temp_map.into_keys().collect();
    let set_pod_names: HashSet<String> = HashSet::from_iter(pod_names);
    let deleted_pod: HashSet<&String> = set_pod_names.difference(&all_pod_names).collect();
    println!("Deleted Pod: {:?}", deleted_pod);

    for pod in deleted_pod {
    
        pod_state().lock().unwrap().get(&namespace.to_owned()).expect("Internal State Error").lock().unwrap().remove(pod);
    } 
   
    Ok(error_pod)
}


async fn prepare_email(podmonitor: Arc<PodMonitor>, error_pods: HashMap<String, (Vec<String>, Vec<String>, String)>) -> Result<(), Error> {

    println!("Setting Up Email...");
    println!("{:?}", error_pods);
    let mut msg = String::new();
    for (key, value) in error_pods.into_iter() {
        let c_status = value.0.join(",");
        let c_reason = value.1.join(",");
        let p_phase = value.2;
        let ns = &podmonitor.spec.target_namespace;
        msg.push_str(&format!("Namespace: {ns}\nPod Name : {key} \nContainers Statuses: {c_status} \nStatus Remark:  {c_reason} \nPOD_STATE: {p_phase}\n\n--------\n"));
    }
    println!("{}", msg);
    utils::send_email(&podmonitor.name_any(), &podmonitor.spec.mail_to, &podmonitor.spec.mail_from, &msg, &podmonitor.spec.smtp_server, podmonitor.spec.smtp_port, &podmonitor.spec.username, &podmonitor.spec.password).await;
    println!("Email Sent!!!");
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

async fn reconcile(podmonitor: Arc<PodMonitor>, context: Arc<ContextData>) -> Result<Action, Error> {
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
            let pods: Api<Pod> = Api::namespaced(client, &target_namespace);
            let error_pods = monitor_pods_in_namespace(pods, &target_namespace).await?;
            if error_pods.is_empty() {
                        return Ok(Action::requeue(Duration::from_secs(10)))
            }
            prepare_email(podmonitor, error_pods).await?;

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
    eprintln!("Error While Monitoring:\n{:?}.\n{:?}", error, podmonitor);
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
