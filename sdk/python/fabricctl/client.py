# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""Python SDK client for zyvor-fabricd REST API."""

import requests
from typing import Optional, List, Dict, Any


class ZyvorFabricClient:
    """Python SDK client for zyvor-fabricd REST API.

    Provides methods for managing VMs, datacenters, clusters, hosts,
    resource pools, DRS, storage, encryption, networking, fault tolerance,
    replication, site recovery, content library, lifecycle management,
    certificates, and system settings.

    Args:
        base_url: Base URL of the zyvor-fabricd API server.
        token: Optional bearer token for authentication.
    """

    def __init__(
        self,
        base_url: str = "http://127.0.0.1:8080",
        token: Optional[str] = None,
    ):
        self.base_url = base_url.rstrip("/")
        self.session = requests.Session()
        if token:
            self.session.headers["Authorization"] = f"Bearer {token}"
        self.session.headers["Content-Type"] = "application/json"

    def _url(self, path: str) -> str:
        return f"{self.base_url}/api{path}"

    def _raw_url(self, path: str) -> str:
        return f"{self.base_url}{path}"

    def _get(self, path: str, params: Optional[Dict] = None) -> Any:
        resp = self.session.get(self._url(path), params=params)
        resp.raise_for_status()
        return resp.json() if resp.content else None

    def _post(self, path: str, data: Optional[Dict] = None) -> Any:
        resp = self.session.post(self._url(path), json=data)
        resp.raise_for_status()
        return resp.json() if resp.content else None

    def _put(self, path: str, data: Optional[Dict] = None) -> Any:
        resp = self.session.put(self._url(path), json=data)
        resp.raise_for_status()
        return resp.json() if resp.content else None

    def _delete(self, path: str) -> None:
        resp = self.session.delete(self._url(path))
        resp.raise_for_status()

    def _get_raw(self, path: str, params: Optional[Dict] = None) -> Any:
        resp = self.session.get(self._raw_url(path), params=params)
        resp.raise_for_status()
        return resp.text if resp.content else None

    # =========================================================================
    # VM Management
    # =========================================================================

    def list_vms(self) -> List[Dict]:
        """List all VMs."""
        return self._get("/vms")

    def get_vm(self, name: str) -> Dict:
        """Get details of a specific VM by name."""
        return self._get(f"/vms/{name}")

    def create_vm(
        self,
        name: str,
        image: str,
        cpus: int,
        memory: int,
        disk: int = 20,
        hostname: Optional[str] = None,
        tags: Optional[List[str]] = None,
    ) -> Dict:
        """Create a new VM."""
        data: Dict[str, Any] = {
            "name": name,
            "image": image,
            "cpus": cpus,
            "memory": memory,
        }
        if disk != 20:
            data["disk"] = disk
        if hostname:
            data["hostname"] = hostname
        if tags:
            data["tags"] = tags
        return self._post("/vms", data)

    def delete_vm(self, name: str) -> None:
        """Delete a VM by name."""
        self._delete(f"/vms/{name}")

    def start_vm(self, name: str) -> None:
        """Start a stopped VM."""
        self._post(f"/vms/{name}/start")

    def stop_vm(self, name: str) -> None:
        """Stop a running VM."""
        self._post(f"/vms/{name}/stop")

    def restart_vm(self, name: str) -> None:
        """Restart a VM."""
        self._post(f"/vms/{name}/restart")

    def get_vm_metrics(self, name: str) -> Dict:
        """Get CPU, memory, disk, and network metrics for a VM."""
        return self._get(f"/vms/{name}/metrics")

    def clone_vm(
        self,
        source_name: str,
        target_name: str,
        include_snapshots: bool = False,
        linked_clone: bool = False,
    ) -> None:
        """Clone a VM."""
        self._post(
            f"/vms/{source_name}/clone",
            {
                "target_name": target_name,
                "include_snapshots": include_snapshots,
                "linked_clone": linked_clone,
            },
        )

    def configure_cloud_init(self, vm_name: str, config: Dict) -> Dict:
        """Configure cloud-init for a VM."""
        return self._post(f"/vms/{vm_name}/cloud-init", config)

    def add_tag(self, vm_name: str, tag: str) -> None:
        """Add a tag to a VM."""
        self._post(f"/vms/{vm_name}/tags", {"tag": tag})

    def remove_tag(self, vm_name: str, tag: str) -> None:
        """Remove a tag from a VM."""
        self._delete(f"/vms/{vm_name}/tags/{tag}")

    def update_tags(self, vm_name: str, tags: List[str]) -> None:
        """Replace all tags on a VM."""
        self._put(f"/vms/{vm_name}/tags", {"tags": tags})

    # =========================================================================
    # Template Management
    # =========================================================================

    def list_templates(self) -> List[Dict]:
        """List all VM templates."""
        return self._get("/templates")

    def create_template(
        self, vm_name: str, template_name: str, description: str = ""
    ) -> None:
        """Create a template from an existing VM."""
        self._post(
            f"/vms/{vm_name}/template",
            {"template_name": template_name, "description": description},
        )

    def create_vm_from_template(
        self, template_name: str, vm_name: str
    ) -> None:
        """Instantiate a new VM from a template."""
        self._post(
            f"/templates/{template_name}/instantiate",
            {"vm_name": vm_name},
        )

    def delete_template(self, name: str) -> None:
        """Delete a template."""
        self._delete(f"/templates/{name}")

    # =========================================================================
    # Datacenter Management
    # =========================================================================

    def list_datacenters(self) -> List[Dict]:
        """List all datacenters."""
        return self._get("/datacenters")

    def create_datacenter(
        self, name: str, description: Optional[str] = None
    ) -> Dict:
        """Create a new datacenter."""
        data: Dict[str, Any] = {"name": name}
        if description:
            data["description"] = description
        return self._post("/datacenters", data)

    def get_datacenter(self, id: str) -> Dict:
        """Get a datacenter by ID."""
        return self._get(f"/datacenters/{id}")

    def update_datacenter(self, id: str, **kwargs) -> Dict:
        """Update a datacenter. Accepts name, description, status."""
        return self._put(f"/datacenters/{id}", kwargs)

    def delete_datacenter(self, id: str) -> None:
        """Delete a datacenter by ID."""
        self._delete(f"/datacenters/{id}")

    def get_datacenter_summary(self, id: str) -> Dict:
        """Get an aggregate summary for a datacenter."""
        return self._get(f"/datacenters/{id}/summary")

    # =========================================================================
    # Cluster Management
    # =========================================================================

    def list_clusters(self, datacenter_id: Optional[str] = None) -> List[Dict]:
        """List all clusters, optionally filtered by datacenter."""
        params = {}
        if datacenter_id:
            params["datacenter_id"] = datacenter_id
        return self._get("/clusters", params=params)

    def create_cluster(
        self,
        name: str,
        datacenter_id: str,
        ha_enabled: bool = True,
        drs_enabled: bool = True,
        description: Optional[str] = None,
        drs_mode: Optional[str] = None,
        evc_mode: Optional[str] = None,
    ) -> Dict:
        """Create a new cluster."""
        data: Dict[str, Any] = {
            "name": name,
            "datacenter_id": datacenter_id,
            "ha_enabled": ha_enabled,
            "drs_enabled": drs_enabled,
        }
        if description:
            data["description"] = description
        if drs_mode:
            data["drs_mode"] = drs_mode
        if evc_mode:
            data["evc_mode"] = evc_mode
        return self._post("/clusters", data)

    def get_cluster(self, id: str) -> Dict:
        """Get a cluster by ID."""
        return self._get(f"/clusters/{id}")

    def update_cluster(self, id: str, **kwargs) -> Dict:
        """Update a cluster. Accepts name, description, ha_enabled, drs_enabled, etc."""
        return self._put(f"/clusters/{id}", kwargs)

    def delete_cluster(self, id: str) -> None:
        """Delete a cluster by ID."""
        self._delete(f"/clusters/{id}")

    # =========================================================================
    # Host Management
    # =========================================================================

    def list_hosts(self, cluster_id: Optional[str] = None) -> List[Dict]:
        """List all hosts, optionally filtered by cluster."""
        params = {}
        if cluster_id:
            params["cluster_id"] = cluster_id
        return self._get("/hosts", params=params)

    def register_host(
        self,
        hostname: str,
        address: str,
        cluster_id: str,
        cpus: int,
        memory_mb: int,
        agent_version: Optional[str] = None,
    ) -> Dict:
        """Register a new host."""
        data: Dict[str, Any] = {
            "hostname": hostname,
            "address": address,
            "cluster_id": cluster_id,
            "cpus": cpus,
            "memory_mb": memory_mb,
        }
        if agent_version:
            data["agent_version"] = agent_version
        return self._post("/hosts", data)

    def get_host(self, id: str) -> Dict:
        """Get a host by ID."""
        return self._get(f"/hosts/{id}")

    def update_host(self, id: str, **kwargs) -> Dict:
        """Update host properties."""
        return self._put(f"/hosts/{id}", kwargs)

    def remove_host(self, id: str) -> None:
        """Remove a host by ID."""
        self._delete(f"/hosts/{id}")

    def host_enter_maintenance(self, id: str) -> None:
        """Put a host into maintenance mode."""
        self._post(f"/hosts/{id}/maintenance/enter")

    def host_exit_maintenance(self, id: str) -> None:
        """Take a host out of maintenance mode."""
        self._post(f"/hosts/{id}/maintenance/exit")

    # =========================================================================
    # Resource Pools
    # =========================================================================

    def list_resource_pools(
        self, cluster_id: Optional[str] = None
    ) -> List[Dict]:
        """List resource pools, optionally filtered by cluster."""
        params = {}
        if cluster_id:
            params["cluster_id"] = cluster_id
        return self._get("/resource-pools", params=params)

    def create_resource_pool(
        self, name: str, cluster_id: str, **kwargs
    ) -> Dict:
        """Create a new resource pool."""
        data: Dict[str, Any] = {"name": name, "cluster_id": cluster_id}
        data.update(kwargs)
        return self._post("/resource-pools", data)

    def get_resource_pool(self, id: str) -> Dict:
        """Get a resource pool by ID."""
        return self._get(f"/resource-pools/{id}")

    def update_resource_pool(self, id: str, **kwargs) -> Dict:
        """Update a resource pool."""
        return self._put(f"/resource-pools/{id}", kwargs)

    def delete_resource_pool(self, id: str) -> None:
        """Delete a resource pool."""
        self._delete(f"/resource-pools/{id}")

    def get_resource_pool_summary(self, id: str) -> Dict:
        """Get a summary for a resource pool."""
        return self._get(f"/resource-pools/{id}/summary")

    def assign_vm_to_pool(self, pool_id: str, vm_name: str) -> None:
        """Assign a VM to a resource pool."""
        self._post(f"/resource-pools/{pool_id}/vms", {"vm_name": vm_name})

    def unassign_vm_from_pool(self, pool_id: str, vm_name: str) -> None:
        """Remove a VM from a resource pool."""
        self._delete(f"/resource-pools/{pool_id}/vms/{vm_name}")

    def check_pool_admission(
        self, pool_id: str, cpu_mhz: int, memory_mb: int
    ) -> Dict:
        """Check if a resource pool can admit a workload."""
        return self._post(
            f"/resource-pools/{pool_id}/check-admission",
            {"cpu_mhz": cpu_mhz, "memory_mb": memory_mb},
        )

    # =========================================================================
    # DRS (Distributed Resource Scheduler)
    # =========================================================================

    def configure_drs(self, config: Dict) -> None:
        """Configure DRS for a cluster."""
        self._put("/drs/config", config)

    def get_drs_config(self, cluster_id: str) -> Dict:
        """Get DRS configuration for a cluster."""
        return self._get("/drs/config", params={"cluster_id": cluster_id})

    def compute_placement(self, request: Dict) -> Dict:
        """Compute optimal VM placement."""
        return self._post("/drs/placement", request)

    def analyze_balance(self, cluster_id: str) -> Dict:
        """Analyze cluster balance."""
        return self._get("/drs/balance", params={"cluster_id": cluster_id})

    def generate_recommendations(self, cluster_id: str) -> List[Dict]:
        """Generate migration recommendations for a cluster."""
        return self._post(
            "/drs/recommendations/generate",
            {"cluster_id": cluster_id},
        )

    def list_recommendations(
        self, cluster_id: Optional[str] = None
    ) -> List[Dict]:
        """List migration recommendations."""
        params = {}
        if cluster_id:
            params["cluster_id"] = cluster_id
        return self._get("/drs/recommendations", params=params)

    def approve_recommendation(self, id: str) -> Dict:
        """Approve a migration recommendation."""
        return self._post(f"/drs/recommendations/{id}/approve")

    def reject_recommendation(self, id: str) -> None:
        """Reject a migration recommendation."""
        self._post(f"/drs/recommendations/{id}/reject")

    # =========================================================================
    # Affinity Rules
    # =========================================================================

    def list_affinity_rules(
        self, cluster_id: Optional[str] = None
    ) -> List[Dict]:
        """List affinity rules, optionally filtered by cluster."""
        params = {}
        if cluster_id:
            params["cluster_id"] = cluster_id
        return self._get("/drs/affinity-rules", params=params)

    def create_affinity_rule(self, rule: Dict) -> Dict:
        """Create a new affinity or anti-affinity rule."""
        return self._post("/drs/affinity-rules", rule)

    def update_affinity_rule(self, id: str, rule: Dict) -> Dict:
        """Update an affinity rule."""
        return self._put(f"/drs/affinity-rules/{id}", rule)

    def delete_affinity_rule(self, id: str) -> None:
        """Delete an affinity rule."""
        self._delete(f"/drs/affinity-rules/{id}")

    # =========================================================================
    # Storage (local pools)
    # =========================================================================

    def list_storage_pools(self) -> List[Dict]:
        """List all local storage pools."""
        return self._get("/storage/pools")

    def get_storage_pool(self, name: str) -> Dict:
        """Get a storage pool by name."""
        return self._get(f"/storage/pools/{name}")

    def create_local_storage_pool(self, pool: Dict) -> Dict:
        """Create a local storage pool."""
        return self._post("/storage/pools/local", pool)

    def create_nfs_storage_pool(self, pool: Dict) -> Dict:
        """Create an NFS storage pool."""
        return self._post("/storage/pools/nfs", pool)

    def delete_storage_pool(self, name: str) -> None:
        """Delete a storage pool."""
        self._delete(f"/storage/pools/{name}")

    def start_storage_pool(self, name: str) -> None:
        """Start a storage pool."""
        self._post(f"/storage/pools/{name}/start")

    def stop_storage_pool(self, name: str) -> None:
        """Stop a storage pool."""
        self._post(f"/storage/pools/{name}/stop")

    def get_storage_pool_health(self, name: str) -> Dict:
        """Get the health status of a storage pool."""
        return self._get(f"/storage/pools/{name}/health")

    def get_storage_pool_stats(self, name: str) -> Dict:
        """Get statistics for a storage pool."""
        return self._get(f"/storage/pools/{name}/stats")

    def refresh_storage_pool_stats(self, name: str) -> None:
        """Refresh statistics for a storage pool."""
        self._post(f"/storage/pools/{name}/refresh")

    # =========================================================================
    # Distributed Storage
    # =========================================================================

    def list_distributed_storage_pools(self) -> List[Dict]:
        """List all distributed storage pools."""
        return self._get("/distributed-storage/pools")

    def create_distributed_storage_pool(self, pool: Dict) -> Dict:
        """Create a distributed storage pool."""
        return self._post("/distributed-storage/pools", pool)

    def delete_distributed_storage_pool(self, id: str) -> None:
        """Delete a distributed storage pool."""
        self._delete(f"/distributed-storage/pools/{id}")

    def list_storage_policies(self) -> List[Dict]:
        """List all storage policies."""
        return self._get("/distributed-storage/policies")

    def create_storage_policy(self, policy: Dict) -> Dict:
        """Create a new storage policy."""
        return self._post("/distributed-storage/policies", policy)

    def delete_storage_policy(self, id: str) -> None:
        """Delete a storage policy."""
        self._delete(f"/distributed-storage/policies/{id}")

    def start_storage_migration(
        self,
        vm_id: str,
        source_pool_id: str,
        target_pool_id: str,
        policy_id: Optional[str] = None,
    ) -> Dict:
        """Start a storage migration for a VM."""
        data: Dict[str, Any] = {
            "vm_id": vm_id,
            "source_pool_id": source_pool_id,
            "target_pool_id": target_pool_id,
        }
        if policy_id:
            data["policy_id"] = policy_id
        return self._post("/distributed-storage/migrations", data)

    def list_storage_migrations(
        self, status: Optional[str] = None
    ) -> List[Dict]:
        """List storage migrations, optionally filtered by status."""
        params = {}
        if status:
            params["status"] = status
        return self._get("/distributed-storage/migrations", params=params)

    def check_storage_compliance(
        self, vm_id: str, policy_id: str
    ) -> Dict:
        """Check VM compliance against a storage policy."""
        return self._post(
            "/distributed-storage/compliance/check",
            {"vm_id": vm_id, "policy_id": policy_id},
        )

    def list_datastore_clusters(self) -> List[Dict]:
        """List all datastore clusters."""
        return self._get("/distributed-storage/datastore-clusters")

    def create_datastore_cluster(self, cluster: Dict) -> Dict:
        """Create a datastore cluster."""
        return self._post("/distributed-storage/datastore-clusters", cluster)

    # =========================================================================
    # Encryption
    # =========================================================================

    def list_encryption_providers(self) -> List[Dict]:
        """List all encryption key providers."""
        return self._get("/encryption/providers")

    def register_encryption_provider(self, provider: Dict) -> Dict:
        """Register a new encryption key provider."""
        return self._post("/encryption/providers", provider)

    def remove_encryption_provider(self, id: str) -> None:
        """Remove an encryption key provider."""
        self._delete(f"/encryption/providers/{id}")

    def list_encryption_policies(self) -> List[Dict]:
        """List all encryption policies."""
        return self._get("/encryption/policies")

    def create_encryption_policy(self, policy: Dict) -> Dict:
        """Create a new encryption policy."""
        return self._post("/encryption/policies", policy)

    def encrypt_vm(self, vm_name: str, policy_id: str) -> Dict:
        """Encrypt a VM with a given policy."""
        return self._post(
            f"/encryption/vms/{vm_name}/encrypt",
            {"policy_id": policy_id},
        )

    def decrypt_vm(self, vm_name: str) -> None:
        """Decrypt a VM."""
        self._post(f"/encryption/vms/{vm_name}/decrypt")

    def get_vm_encryption_status(self, vm_name: str) -> Dict:
        """Get the encryption status of a VM."""
        return self._get(f"/encryption/vms/{vm_name}/status")

    def list_encrypted_vms(self) -> List[Dict]:
        """List all encrypted VMs."""
        return self._get("/encryption/vms")

    def rotate_vm_encryption_key(self, vm_name: str) -> None:
        """Rotate the encryption key for a VM."""
        self._post(f"/encryption/vms/{vm_name}/rotate-key")

    # =========================================================================
    # Networking
    # =========================================================================

    def list_switches(self) -> List[Dict]:
        """List all distributed switches."""
        return self._get("/networking/switches")

    def create_switch(self, switch: Dict) -> Dict:
        """Create a distributed switch."""
        return self._post("/networking/switches", switch)

    def delete_switch(self, id: str) -> None:
        """Delete a distributed switch."""
        self._delete(f"/networking/switches/{id}")

    def list_port_groups(
        self, switch_id: Optional[str] = None
    ) -> List[Dict]:
        """List port groups, optionally filtered by switch."""
        params = {}
        if switch_id:
            params["switch_id"] = switch_id
        return self._get("/networking/port-groups", params=params)

    def create_port_group(self, port_group: Dict) -> Dict:
        """Create a port group."""
        return self._post("/networking/port-groups", port_group)

    def list_firewall_rules(
        self, security_group_id: Optional[str] = None
    ) -> List[Dict]:
        """List firewall rules, optionally filtered by security group."""
        params = {}
        if security_group_id:
            params["security_group_id"] = security_group_id
        return self._get("/networking/firewall-rules", params=params)

    def create_firewall_rule(self, rule: Dict) -> Dict:
        """Create a firewall rule."""
        return self._post("/networking/firewall-rules", rule)

    def delete_firewall_rule(self, id: str) -> None:
        """Delete a firewall rule."""
        self._delete(f"/networking/firewall-rules/{id}")

    def list_security_groups(self) -> List[Dict]:
        """List all security groups."""
        return self._get("/networking/security-groups")

    def create_security_group(self, group: Dict) -> Dict:
        """Create a security group."""
        return self._post("/networking/security-groups", group)

    def list_overlay_networks(self) -> List[Dict]:
        """List all overlay networks."""
        return self._get("/networking/overlays")

    def create_overlay_network(self, overlay: Dict) -> Dict:
        """Create an overlay network."""
        return self._post("/networking/overlays", overlay)

    def list_load_balancers(self) -> List[Dict]:
        """List all load balancers."""
        return self._get("/networking/load-balancers")

    def create_load_balancer(self, lb: Dict) -> Dict:
        """Create a load balancer."""
        return self._post("/networking/load-balancers", lb)

    # =========================================================================
    # Fault Tolerance
    # =========================================================================

    def enable_ft(
        self, vm_name: str, primary_host: str, secondary_host: str
    ) -> Dict:
        """Enable fault tolerance for a VM."""
        return self._post(
            f"/ft/vms/{vm_name}/enable",
            {
                "primary_host_id": primary_host,
                "secondary_host_id": secondary_host,
            },
        )

    def disable_ft(self, vm_name: str) -> None:
        """Disable fault tolerance for a VM."""
        self._post(f"/ft/vms/{vm_name}/disable")

    def get_ft_config(self, vm_name: str) -> Dict:
        """Get the fault tolerance configuration for a VM."""
        return self._get(f"/ft/vms/{vm_name}")

    def list_ft_vms(self) -> List[Dict]:
        """List all VMs with fault tolerance enabled."""
        return self._get("/ft/vms")

    def trigger_failover(self, vm_name: str) -> Dict:
        """Trigger a failover for a fault-tolerant VM."""
        return self._post(f"/ft/vms/{vm_name}/failover")

    def test_failover(self, vm_name: str) -> Dict:
        """Run a non-disruptive test failover."""
        return self._post(f"/ft/vms/{vm_name}/test-failover")

    # =========================================================================
    # Replication
    # =========================================================================

    def list_replication_sites(self) -> List[Dict]:
        """List all replication sites."""
        return self._get("/replication/sites")

    def register_replication_site(self, site: Dict) -> Dict:
        """Register a new replication site."""
        return self._post("/replication/sites", site)

    def remove_replication_site(self, id: str) -> None:
        """Remove a replication site."""
        self._delete(f"/replication/sites/{id}")

    def configure_replication(self, config: Dict) -> Dict:
        """Configure replication for a VM."""
        return self._post("/replication/configs", config)

    def list_replications(
        self, site_id: Optional[str] = None
    ) -> List[Dict]:
        """List replication configurations."""
        params = {}
        if site_id:
            params["site_id"] = site_id
        return self._get("/replication/configs", params=params)

    def pause_replication(self, id: str) -> None:
        """Pause a replication."""
        self._post(f"/replication/configs/{id}/pause")

    def resume_replication(self, id: str) -> None:
        """Resume a paused replication."""
        self._post(f"/replication/configs/{id}/resume")

    def get_replication_health(self) -> Dict:
        """Get overall replication health summary."""
        return self._get("/replication/health")

    # =========================================================================
    # Site Recovery
    # =========================================================================

    def list_recovery_plans(self) -> List[Dict]:
        """List all recovery plans."""
        return self._get("/site-recovery/plans")

    def create_recovery_plan(self, plan: Dict) -> Dict:
        """Create a new recovery plan."""
        return self._post("/site-recovery/plans", plan)

    def get_recovery_plan(self, id: str) -> Dict:
        """Get a recovery plan by ID."""
        return self._get(f"/site-recovery/plans/{id}")

    def update_recovery_plan(self, id: str, plan: Dict) -> Dict:
        """Update a recovery plan."""
        return self._put(f"/site-recovery/plans/{id}", plan)

    def delete_recovery_plan(self, id: str) -> None:
        """Delete a recovery plan."""
        self._delete(f"/site-recovery/plans/{id}")

    def execute_planned_migration(self, plan_id: str) -> Dict:
        """Execute a planned (graceful) migration."""
        return self._post(f"/site-recovery/plans/{plan_id}/planned-migration")

    def execute_disaster_recovery(self, plan_id: str) -> Dict:
        """Execute a disaster recovery failover."""
        return self._post(f"/site-recovery/plans/{plan_id}/disaster-recovery")

    def execute_test_failover(self, plan_id: str) -> Dict:
        """Execute a non-disruptive test failover."""
        return self._post(f"/site-recovery/plans/{plan_id}/test-failover")

    def list_recovery_executions(
        self, plan_id: Optional[str] = None
    ) -> List[Dict]:
        """List recovery executions."""
        params = {}
        if plan_id:
            params["plan_id"] = plan_id
        return self._get("/site-recovery/executions", params=params)

    def get_dr_dashboard(self) -> Dict:
        """Get the disaster recovery dashboard."""
        return self._get("/site-recovery/dashboard")

    # =========================================================================
    # Content Library
    # =========================================================================

    def list_libraries(self) -> List[Dict]:
        """List all content libraries."""
        return self._get("/content-library/libraries")

    def create_library(self, library: Dict) -> Dict:
        """Create a new content library."""
        return self._post("/content-library/libraries", library)

    def get_library(self, id: str) -> Dict:
        """Get a content library by ID."""
        return self._get(f"/content-library/libraries/{id}")

    def delete_library(self, id: str) -> None:
        """Delete a content library."""
        self._delete(f"/content-library/libraries/{id}")

    def sync_library(self, id: str) -> None:
        """Synchronize a subscribed content library."""
        self._post(f"/content-library/libraries/{id}/sync")

    def list_library_items(self, library_id: str) -> List[Dict]:
        """List items in a content library."""
        return self._get(f"/content-library/libraries/{library_id}/items")

    def add_library_item(self, library_id: str, item: Dict) -> Dict:
        """Add an item to a content library."""
        return self._post(
            f"/content-library/libraries/{library_id}/items", item
        )

    def search_library_items(self, query: str) -> List[Dict]:
        """Search for library items by name."""
        return self._get(
            "/content-library/items/search", params={"query": query}
        )

    # =========================================================================
    # Lifecycle Manager
    # =========================================================================

    def list_baselines(self) -> List[Dict]:
        """List all update baselines."""
        return self._get("/lifecycle/baselines")

    def create_baseline(self, baseline: Dict) -> Dict:
        """Create a new update baseline."""
        return self._post("/lifecycle/baselines", baseline)

    def get_baseline(self, id: str) -> Dict:
        """Get an update baseline by ID."""
        return self._get(f"/lifecycle/baselines/{id}")

    def delete_baseline(self, id: str) -> None:
        """Delete an update baseline."""
        self._delete(f"/lifecycle/baselines/{id}")

    def scan_host_compliance(
        self, host_id: str, baseline_id: str
    ) -> Dict:
        """Scan a host for compliance against a baseline."""
        return self._post(
            "/lifecycle/compliance/scan",
            {"host_id": host_id, "baseline_id": baseline_id},
        )

    def get_compliance_status(self, host_id: str) -> List[Dict]:
        """Get compliance statuses for a host."""
        return self._get(f"/lifecycle/compliance/{host_id}")

    def create_remediation(
        self, host_id: str, baseline_id: str
    ) -> Dict:
        """Create a remediation task for a host."""
        return self._post(
            "/lifecycle/remediations",
            {"host_id": host_id, "baseline_id": baseline_id},
        )

    def create_rolling_update(self, plan: Dict) -> Dict:
        """Create a rolling update plan."""
        return self._post("/lifecycle/rolling-updates", plan)

    def get_rolling_update(self, id: str) -> Dict:
        """Get a rolling update plan by ID."""
        return self._get(f"/lifecycle/rolling-updates/{id}")

    def start_rolling_update(self, id: str) -> None:
        """Start a rolling update."""
        self._post(f"/lifecycle/rolling-updates/{id}/start")

    # =========================================================================
    # Certificates
    # =========================================================================

    def list_certificates(
        self, component: Optional[str] = None
    ) -> List[Dict]:
        """List certificates, optionally filtered by component."""
        params = {}
        if component:
            params["component"] = component
        return self._get("/certificates/certs", params=params)

    def get_certificate(self, id: str) -> Dict:
        """Get a certificate by ID."""
        return self._get(f"/certificates/certs/{id}")

    def issue_certificate(self, request: Dict) -> Dict:
        """Issue a new certificate from a request."""
        return self._post("/certificates/certs", request)

    def revoke_certificate(self, id: str) -> None:
        """Revoke an active certificate."""
        self._post(f"/certificates/certs/{id}/revoke")

    def renew_certificate(self, id: str) -> Dict:
        """Renew a certificate."""
        return self._post(f"/certificates/certs/{id}/renew")

    def list_certificate_authorities(self) -> List[Dict]:
        """List all certificate authorities."""
        return self._get("/certificates/cas")

    def create_certificate_authority(self, ca: Dict) -> Dict:
        """Register a certificate authority."""
        return self._post("/certificates/cas", ca)

    def submit_certificate_request(self, request: Dict) -> Dict:
        """Submit a certificate signing request."""
        return self._post("/certificates/requests", request)

    def approve_certificate_request(self, id: str) -> Dict:
        """Approve a pending certificate request."""
        return self._post(f"/certificates/requests/{id}/approve")

    def reject_certificate_request(self, id: str) -> None:
        """Reject a pending certificate request."""
        self._post(f"/certificates/requests/{id}/reject")

    def get_cert_health_dashboard(self) -> Dict:
        """Get the certificate infrastructure health dashboard."""
        return self._get("/certificates/dashboard")

    # =========================================================================
    # System Resources
    # =========================================================================

    def get_cpu_topology(self) -> Dict:
        """Get CPU topology information."""
        return self._get("/system/cpu/topology")

    def set_cpu_pinning(self, vm_name: str, pinning: Dict) -> Dict:
        """Set CPU pinning for a VM."""
        return self._post(f"/vms/{vm_name}/cpu/pin", pinning)

    def remove_cpu_pinning(self, vm_name: str) -> None:
        """Remove CPU pinning from a VM."""
        self._delete(f"/vms/{vm_name}/cpu/pin")

    def get_cpu_affinity(self, vm_name: str) -> Dict:
        """Get CPU affinity information for a VM."""
        return self._get(f"/vms/{vm_name}/cpu/affinity")

    def get_numa_topology(self) -> Dict:
        """Get NUMA topology information."""
        return self._get("/system/numa/topology")

    def get_numa_node(self, node_id: int) -> Dict:
        """Get information about a specific NUMA node."""
        return self._get(f"/system/numa/nodes/{node_id}")

    def get_numa_placement(self) -> Dict:
        """Get NUMA placement information."""
        return self._get("/system/numa/placement")

    def set_memory_limit(self, vm_name: str, limit: Dict) -> Dict:
        """Set memory limit for a VM."""
        return self._put(f"/vms/{vm_name}/memory/limit", limit)

    def get_memory_usage(self, vm_name: str) -> Dict:
        """Get memory usage for a VM."""
        return self._get(f"/vms/{vm_name}/memory/usage")

    def set_memory_ballooning(self, vm_name: str, config: Dict) -> Dict:
        """Configure memory ballooning for a VM."""
        return self._post(f"/vms/{vm_name}/memory/balloon", config)

    def get_hugepage_stats(self, size: Optional[int] = None) -> Dict:
        """Get hugepage statistics."""
        params = {}
        if size:
            params["size"] = size
        return self._get("/system/memory/hugepages", params=params)

    def allocate_hugepages(self, config: Dict) -> Dict:
        """Allocate hugepages."""
        return self._post("/system/memory/hugepages", config)

    def get_system_memory(self) -> Dict:
        """Get system memory information."""
        return self._get("/system/memory")

    # =========================================================================
    # Firmware
    # =========================================================================

    def get_firmware_status(self, vm_name: str) -> Dict:
        """Get firmware status for a VM."""
        return self._get(f"/vms/{vm_name}/firmware/status")

    def enable_uefi(self, vm_name: str, config: Optional[Dict] = None) -> Dict:
        """Enable UEFI firmware for a VM."""
        return self._post(f"/vms/{vm_name}/firmware/uefi", config or {})

    def enable_secureboot(
        self, vm_name: str, config: Optional[Dict] = None
    ) -> Dict:
        """Enable secure boot for a VM."""
        return self._post(
            f"/vms/{vm_name}/firmware/secureboot", config or {}
        )

    def disable_secureboot(self, vm_name: str) -> None:
        """Disable secure boot for a VM."""
        self._delete(f"/vms/{vm_name}/firmware/secureboot")

    def reset_nvram(self, vm_name: str) -> None:
        """Reset NVRAM for a VM."""
        self._post(f"/vms/{vm_name}/firmware/reset")

    def get_firmware_capabilities(self) -> Dict:
        """Get system firmware capabilities."""
        return self._get("/system/firmware/capabilities")

    # =========================================================================
    # Notifications
    # =========================================================================

    def list_notification_channels(self) -> List[Dict]:
        """List all notification channels."""
        return self._get("/notifications/channels")

    def create_notification_channel(self, channel: Dict) -> Dict:
        """Create a notification channel."""
        return self._post("/notifications/channels", channel)

    def update_notification_channel(self, id: str, channel: Dict) -> Dict:
        """Update a notification channel."""
        return self._put(f"/notifications/channels/{id}", channel)

    def delete_notification_channel(self, id: str) -> None:
        """Delete a notification channel."""
        self._delete(f"/notifications/channels/{id}")

    def test_notification_channel(self, id: str) -> Dict:
        """Send a test notification to a channel."""
        return self._post(f"/notifications/channels/{id}/test")

    def list_notification_rules(self) -> List[Dict]:
        """List all notification rules."""
        return self._get("/notifications/rules")

    def create_notification_rule(self, rule: Dict) -> Dict:
        """Create a notification rule."""
        return self._post("/notifications/rules", rule)

    def get_notification_history(self, limit: int = 50) -> List[Dict]:
        """Get notification history."""
        return self._get(
            "/notifications/history", params={"limit": limit}
        )

    # =========================================================================
    # Quotas
    # =========================================================================

    def list_quotas(self) -> List[Dict]:
        """List all quotas."""
        return self._get("/quotas")

    def create_quota(self, quota: Dict) -> Dict:
        """Create a new quota."""
        return self._post("/quotas", quota)

    def get_quota(self, id: str) -> Dict:
        """Get a quota by ID."""
        return self._get(f"/quotas/{id}")

    def update_quota(self, id: str, quota: Dict) -> Dict:
        """Update a quota."""
        return self._put(f"/quotas/{id}", quota)

    def delete_quota(self, id: str) -> None:
        """Delete a quota."""
        self._delete(f"/quotas/{id}")

    def enable_quota(self, id: str) -> None:
        """Enable a quota."""
        self._post(f"/quotas/{id}/enable")

    def disable_quota(self, id: str) -> None:
        """Disable a quota."""
        self._post(f"/quotas/{id}/disable")

    def get_quota_usage(self, id: str) -> Dict:
        """Get usage for a specific quota."""
        return self._get(f"/quotas/{id}/usage")

    def get_all_quota_usage(self) -> List[Dict]:
        """Get usage for all quotas."""
        return self._get("/quotas/usage")

    # =========================================================================
    # Schedules
    # =========================================================================

    def list_schedules(self) -> List[Dict]:
        """List all VM schedules."""
        return self._get("/schedules")

    def create_schedule(self, schedule: Dict) -> Dict:
        """Create a new VM schedule."""
        return self._post("/schedules", schedule)

    def get_schedule(self, id: str) -> Dict:
        """Get a schedule by ID."""
        return self._get(f"/schedules/{id}")

    def update_schedule(self, id: str, schedule: Dict) -> Dict:
        """Update a schedule."""
        return self._put(f"/schedules/{id}", schedule)

    def delete_schedule(self, id: str) -> None:
        """Delete a schedule."""
        self._delete(f"/schedules/{id}")

    def enable_schedule(self, id: str) -> None:
        """Enable a schedule."""
        self._post(f"/schedules/{id}/enable")

    def disable_schedule(self, id: str) -> None:
        """Disable a schedule."""
        self._post(f"/schedules/{id}/disable")

    def run_schedule_now(self, id: str) -> Dict:
        """Run a schedule immediately."""
        return self._post(f"/schedules/{id}/run")

    def get_schedule_history(self, id: str) -> List[Dict]:
        """Get execution history for a schedule."""
        return self._get(f"/schedules/{id}/history")

    # =========================================================================
    # Audit
    # =========================================================================

    def list_audit_logs(self, **params) -> List[Dict]:
        """List audit log entries. Supports pagination and filtering."""
        return self._get("/audit/logs", params=params if params else None)

    def get_audit_log(self, id: str) -> Dict:
        """Get a specific audit log entry."""
        return self._get(f"/audit/logs/{id}")

    def get_audit_stats(self) -> Dict:
        """Get audit log statistics."""
        return self._get("/audit/stats")

    # =========================================================================
    # Analytics
    # =========================================================================

    def get_vm_performance(self, name: str) -> Dict:
        """Get performance analytics for a VM."""
        return self._get(f"/analytics/vms/{name}")

    def get_system_performance(self) -> Dict:
        """Get overall system performance analytics."""
        return self._get("/analytics/system")

    def get_performance_insights(self) -> Dict:
        """Get performance insights and recommendations."""
        return self._get("/analytics/insights")

    def get_top_vms_by_resource(self, **params) -> List[Dict]:
        """Get top VMs by resource usage."""
        return self._get("/analytics/top", params=params if params else None)

    def get_resource_utilization(self) -> Dict:
        """Get resource utilization statistics."""
        return self._get("/analytics/utilization")

    # =========================================================================
    # Backups
    # =========================================================================

    def list_backups(self, **params) -> List[Dict]:
        """List all backups."""
        return self._get("/backups", params=params if params else None)

    def create_backup(self, backup: Dict) -> Dict:
        """Create a new backup."""
        return self._post("/backups", backup)

    def get_backup(self, id: str) -> Dict:
        """Get a backup by ID."""
        return self._get(f"/backups/{id}")

    def delete_backup(self, id: str) -> None:
        """Delete a backup."""
        self._delete(f"/backups/{id}")

    def restore_backup(self, restore_config: Dict) -> Dict:
        """Restore from a backup."""
        return self._post("/backups/restore", restore_config)

    def list_backup_jobs(self) -> List[Dict]:
        """List backup jobs."""
        return self._get("/backups/jobs")

    def list_backup_policies(self) -> List[Dict]:
        """List backup policies."""
        return self._get("/backups/policies")

    def create_backup_policy(self, policy: Dict) -> Dict:
        """Create a backup policy."""
        return self._post("/backups/policies", policy)

    def delete_backup_policy(self, id: str) -> None:
        """Delete a backup policy."""
        self._delete(f"/backups/policies/{id}")

    def enable_backup_policy(self, id: str) -> None:
        """Enable a backup policy."""
        self._post(f"/backups/policies/{id}/enable")

    def disable_backup_policy(self, id: str) -> None:
        """Disable a backup policy."""
        self._post(f"/backups/policies/{id}/disable")

    def get_backup_stats(self) -> Dict:
        """Get backup statistics."""
        return self._get("/backups/stats")

    # =========================================================================
    # Settings
    # =========================================================================

    def get_settings(self) -> Dict:
        """Get current daemon settings."""
        return self._get("/settings")

    def update_settings(self, settings: Dict) -> Dict:
        """Update daemon settings."""
        return self._put("/settings", settings)

    # =========================================================================
    # System (top-level routes outside /api)
    # =========================================================================

    def health(self) -> str:
        """Check daemon health."""
        return self._get_raw("/health")

    def metrics(self) -> str:
        """Get Prometheus metrics."""
        return self._get_raw("/metrics")
