use apiexts::CustomResourceDefinition;
use k8s_openapi::{apiextensions_apiserver::pkg::apis::apiextensions::v1 as apiexts, serde};
use kube::{CustomResource, CustomResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "kk.dev",
    version = "v1",
    kind = "PodMonitor",
    plural = "podmonitors",
    namespaced
)]
#[kube(status = "PodMonitorStatus")]
pub struct PodMonitorSpec {
    pub name: String,
    pub target_namespace: String,
    pub smtp_server: String,
    pub smtp_port: u16,
    pub mail_to: String,
    pub mail_from: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
pub struct PodMonitorStatus {
    pub is_ok: bool,
}
