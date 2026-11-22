// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct Cloud {
    inner: Arc<RwLock<Inner>>,
    pub public_url: String,
}

struct Inner {
    tokens: HashMap<String, Token>,
    servers: HashMap<String, Server>,
    images: HashMap<String, Image>,
    networks: HashMap<String, Network>,
    subnets: HashMap<String, Subnet>,
    ports: HashMap<String, Port>,
    volumes: HashMap<String, Volume>,
    flavors: Vec<Flavor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Token {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub project_id: String,
    pub project_name: String,
    pub roles: Vec<String>,
    pub expires_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Flavor {
    pub id: String,
    pub name: String,
    pub vcpus: u32,
    pub ram: u32,
    pub disk: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub status: String,
    pub flavor_id: String,
    pub image_id: String,
    pub project_id: String,
    pub addresses: HashMap<String, Vec<serde_json::Value>>,
    pub created: String,
    pub updated: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Image {
    pub id: String,
    pub name: String,
    pub status: String,
    pub visibility: String,
    pub disk_format: String,
    pub container_format: String,
    pub size: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Network {
    pub id: String,
    pub name: String,
    pub admin_state_up: bool,
    pub shared: bool,
    pub status: String,
    pub project_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subnet {
    pub id: String,
    pub name: String,
    pub network_id: String,
    pub cidr: String,
    pub ip_version: u8,
    pub gateway_ip: String,
    pub project_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Port {
    pub id: String,
    pub name: String,
    pub network_id: String,
    pub mac_address: String,
    pub device_id: String,
    pub status: String,
    pub project_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Volume {
    pub id: String,
    pub name: String,
    pub size: u32,
    pub status: String,
    pub bootable: bool,
    pub attachments: Vec<serde_json::Value>,
    pub project_id: String,
}

impl Cloud {
    pub fn new(public_url: impl Into<String>) -> Self {
        let project = "fabric-default";
        let mut images = HashMap::new();
        let cirros_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaa1";
        images.insert(
            cirros_id.into(),
            Image {
                id: cirros_id.into(),
                name: "cirros".into(),
                status: "active".into(),
                visibility: "public".into(),
                disk_format: "qcow2".into(),
                container_format: "bare".into(),
                size: 20_971_520,
            },
        );
        let mut networks = HashMap::new();
        networks.insert(
            "net-private".into(),
            Network {
                id: "net-private".into(),
                name: "private".into(),
                admin_state_up: true,
                shared: false,
                status: "ACTIVE".into(),
                project_id: project.into(),
            },
        );
        let mut subnets = HashMap::new();
        subnets.insert(
            "subnet-private".into(),
            Subnet {
                id: "subnet-private".into(),
                name: "private-subnet".into(),
                network_id: "net-private".into(),
                cidr: "10.0.0.0/24".into(),
                ip_version: 4,
                gateway_ip: "10.0.0.1".into(),
                project_id: project.into(),
            },
        );
        Self {
            public_url: public_url.into(),
            inner: Arc::new(RwLock::new(Inner {
                tokens: HashMap::new(),
                servers: HashMap::new(),
                images,
                networks,
                subnets,
                ports: HashMap::new(),
                volumes: HashMap::new(),
                flavors: default_flavors(),
            })),
        }
    }

    pub async fn issue_token(&self, user: &str, project: &str) -> Token {
        let token = Token {
            id: format!("tok-{}", Uuid::new_v4()),
            user_id: format!("user-{}", user),
            user_name: user.to_string(),
            project_id: format!("proj-{}", project),
            project_name: project.to_string(),
            roles: vec!["admin".into(), "member".into()],
            expires_at: (Utc::now() + Duration::hours(8)).to_rfc3339(),
        };
        self.inner
            .write()
            .await
            .tokens
            .insert(token.id.clone(), token.clone());
        token
    }

    pub async fn token(&self, id: &str) -> Option<Token> {
        if id.is_empty() {
            return None;
        }
        let tok = self.inner.read().await.tokens.get(id).cloned()?;
        if chrono::DateTime::parse_from_rfc3339(&tok.expires_at)
            .map(|t| t < Utc::now())
            .unwrap_or(true)
        {
            return None;
        }
        Some(tok)
    }

    pub async fn flavors(&self) -> Vec<Flavor> {
        self.inner.read().await.flavors.clone()
    }

    pub async fn flavor(&self, id_or_name: &str) -> Option<Flavor> {
        self.inner
            .read()
            .await
            .flavors
            .iter()
            .find(|f| f.id == id_or_name || f.name == id_or_name)
            .cloned()
    }

    pub async fn list_servers(&self) -> Vec<Server> {
        self.inner.read().await.servers.values().cloned().collect()
    }

    pub async fn get_server(&self, id: &str) -> Option<Server> {
        let g = self.inner.read().await;
        g.servers
            .get(id)
            .cloned()
            .or_else(|| g.servers.values().find(|s| s.name == id).cloned())
    }

    pub async fn create_server(
        &self,
        name: String,
        flavor_id: String,
        image_id: String,
        project_id: String,
        network_id: Option<String>,
    ) -> Server {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let net = network_id.unwrap_or_else(|| "net-private".into());
        let mut addresses = HashMap::new();
        addresses.insert(
            "private".into(),
            vec![serde_json::json!({
                "addr": format!("10.0.0.{}", 10 + (id.as_bytes()[0] % 200)),
                "version": 4,
                "OS-EXT-IPS:type": "fixed"
            })],
        );
        let server = Server {
            id: id.clone(),
            name,
            status: "ACTIVE".into(),
            flavor_id,
            image_id,
            project_id,
            addresses,
            created: now.clone(),
            updated: now,
        };
        let mut port = Port {
            id: format!("port-{id}"),
            name: format!("port-{}", server.name),
            network_id: net,
            mac_address: "fa:16:3e:00:00:01".into(),
            device_id: id.clone(),
            status: "ACTIVE".into(),
            project_id: server.project_id.clone(),
        };
        port.mac_address = format!(
            "fa:16:3e:{:02x}:{:02x}:{:02x}",
            id.as_bytes()[0],
            id.as_bytes()[1],
            id.as_bytes()[2]
        );
        let mut g = self.inner.write().await;
        g.ports.insert(port.id.clone(), port);
        g.servers.insert(id, server.clone());
        server
    }

    pub async fn delete_server(&self, id: &str) -> bool {
        let mut g = self.inner.write().await;
        g.ports.retain(|_, p| p.device_id != id);
        g.servers.remove(id).is_some()
    }

    pub async fn set_server_status(&self, id: &str, status: &str) -> Option<Server> {
        let mut g = self.inner.write().await;
        let s = g.servers.get_mut(id)?;
        s.status = status.to_string();
        s.updated = Utc::now().to_rfc3339();
        Some(s.clone())
    }

    pub async fn list_images(&self) -> Vec<Image> {
        self.inner.read().await.images.values().cloned().collect()
    }

    pub async fn get_image(&self, id: &str) -> Option<Image> {
        let g = self.inner.read().await;
        g.images
            .get(id)
            .cloned()
            .or_else(|| g.images.values().find(|i| i.name == id).cloned())
    }

    pub async fn create_image(&self, name: String, disk_format: String) -> Image {
        let id = Uuid::new_v4().to_string();
        let image = Image {
            id: id.clone(),
            name,
            status: "queued".into(),
            visibility: "private".into(),
            disk_format,
            container_format: "bare".into(),
            size: 0,
        };
        self.inner.write().await.images.insert(id, image.clone());
        image
    }

    pub async fn delete_image(&self, id: &str) -> bool {
        self.inner.write().await.images.remove(id).is_some()
    }

    pub async fn list_networks(&self) -> Vec<Network> {
        self.inner.read().await.networks.values().cloned().collect()
    }

    pub async fn create_network(&self, name: String, project_id: String) -> Network {
        let id = Uuid::new_v4().to_string();
        let net = Network {
            id: id.clone(),
            name,
            admin_state_up: true,
            shared: false,
            status: "ACTIVE".into(),
            project_id,
        };
        self.inner.write().await.networks.insert(id, net.clone());
        net
    }

    pub async fn list_subnets(&self) -> Vec<Subnet> {
        self.inner.read().await.subnets.values().cloned().collect()
    }

    pub async fn create_subnet(
        &self,
        name: String,
        network_id: String,
        cidr: String,
        project_id: String,
    ) -> Subnet {
        let id = Uuid::new_v4().to_string();
        let gw = cidr
            .split('/')
            .next()
            .unwrap_or("10.0.0.0")
            .rsplit('.')
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(".")
            + ".1";
        let subnet = Subnet {
            id: id.clone(),
            name,
            network_id,
            cidr,
            ip_version: 4,
            gateway_ip: gw,
            project_id,
        };
        self.inner.write().await.subnets.insert(id, subnet.clone());
        subnet
    }

    pub async fn list_ports(&self) -> Vec<Port> {
        self.inner.read().await.ports.values().cloned().collect()
    }

    pub async fn create_port(&self, name: String, network_id: String, project_id: String) -> Port {
        let id = Uuid::new_v4().to_string();
        let port = Port {
            id: id.clone(),
            name,
            network_id,
            mac_address: format!(
                "fa:16:3e:{:02x}:{:02x}:{:02x}",
                id.as_bytes()[0],
                id.as_bytes()[1],
                id.as_bytes()[2]
            ),
            device_id: String::new(),
            status: "DOWN".into(),
            project_id,
        };
        self.inner.write().await.ports.insert(id, port.clone());
        port
    }

    pub async fn list_volumes(&self) -> Vec<Volume> {
        self.inner.read().await.volumes.values().cloned().collect()
    }

    pub async fn get_volume(&self, id: &str) -> Option<Volume> {
        self.inner.read().await.volumes.get(id).cloned()
    }

    pub async fn create_volume(&self, name: String, size: u32, project_id: String) -> Volume {
        let id = Uuid::new_v4().to_string();
        let vol = Volume {
            id: id.clone(),
            name,
            size,
            status: "available".into(),
            bootable: false,
            attachments: vec![],
            project_id,
        };
        self.inner.write().await.volumes.insert(id, vol.clone());
        vol
    }

    pub async fn attach_volume(&self, id: &str, server_id: &str) -> Option<Volume> {
        let mut g = self.inner.write().await;
        let vol = g.volumes.get_mut(id)?;
        vol.status = "in-use".into();
        vol.attachments = vec![serde_json::json!({
            "server_id": server_id,
            "attachment_id": Uuid::new_v4().to_string(),
            "device": "/dev/vdb"
        })];
        Some(vol.clone())
    }

    pub async fn detach_volume(&self, id: &str) -> Option<Volume> {
        let mut g = self.inner.write().await;
        let vol = g.volumes.get_mut(id)?;
        vol.status = "available".into();
        vol.attachments.clear();
        Some(vol.clone())
    }
}

fn default_flavors() -> Vec<Flavor> {
    vec![
        Flavor {
            id: "1".into(),
            name: "m1.tiny".into(),
            vcpus: 1,
            ram: 512,
            disk: 1,
        },
        Flavor {
            id: "2".into(),
            name: "m1.small".into(),
            vcpus: 1,
            ram: 2048,
            disk: 20,
        },
        Flavor {
            id: "3".into(),
            name: "m1.medium".into(),
            vcpus: 2,
            ram: 4096,
            disk: 40,
        },
        Flavor {
            id: "4".into(),
            name: "m1.large".into(),
            vcpus: 4,
            ram: 8192,
            disk: 80,
        },
        Flavor {
            id: "5".into(),
            name: "m1.xlarge".into(),
            vcpus: 8,
            ram: 16384,
            disk: 160,
        },
    ]
}
