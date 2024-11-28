use apiexts::CustomResourceDefinition;
use k8s_openapi::{apiextensions_apiserver::pkg::apis::apiextensions::v1 as apiexts, serde};
use kube::{CustomResource, CustomResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub enum TlsOption {
    #[serde(rename = "true")]
    True,
    #[serde(rename = "false")]
    False,
}
impl TlsOption {
    pub fn as_str(&self) -> &str {
        match self {
            TlsOption::True => "true",
            TlsOption::False => "false",
        }
    }
}

impl std::fmt::Display for TlsOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct MailDetails {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub to: String,
    pub from: String,
    pub tls: TlsOption,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct WebHook {
    pub url: String,
}

// #[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
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
    pub target_pods: Option<Vec<String>>,
    pub mail: Option<MailDetails>,
    pub webhook: Option<WebHook>,
}

impl PodMonitorSpec {
    pub fn get_mail_to(&self) -> Option<&String> {
        self.mail.as_ref().map(|mail| &mail.to)
    }

    pub fn get_mail_from(&self) -> Option<&String> {
        self.mail.as_ref().map(|mail| &mail.from)
    }

    pub fn get_smtp_server(&self) -> Option<&String> {
        self.mail.as_ref().map(|mail| &mail.smtp_server)
    }

    pub fn get_smtp_port(&self) -> Option<&u16> {
        self.mail.as_ref().map(|mail| &mail.smtp_port)
    }

    pub fn get_tls(&self) -> Option<&TlsOption> {
        self.mail.as_ref().map(|mail| &mail.tls)
    }

    pub fn get_mail_username(&self) -> Option<&String> {
        self.mail.as_ref().and_then(|mail| mail.username.as_ref())
    }

    pub fn get_mail_password(&self) -> Option<&String> {
        self.mail.as_ref().and_then(|mail| mail.password.as_ref())
    }

    pub fn get_webhook_url(&self) -> Option<&String> {
        self.webhook.as_ref().map(|webhook| &webhook.url)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
pub struct PodMonitorStatus {
    pub is_ok: bool,
}
