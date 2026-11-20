# Ansible Collection for Zyvor Fabric

Ansible collection for automating Zyvor Fabric virtual machine management. Provides modules for VM lifecycle, snapshots, networking, and configuration management.

## Module Structure

Two modules ship today, under `sdk/ansible/plugins/modules/` (not `ansible/plugins/modules/`):

```
sdk/ansible/
  plugins/
    modules/
      zyvor_fabric_vm.py          # Create/delete/start/stop/restart VMs
      zyvor_fabric_datacenter.py  # Manage datacenter resources
```

Snapshot, networking, image, and template modules are not implemented yet.

## Example Playbook

Not yet packaged as a Galaxy collection (no `galaxy.yml`) -- point `ANSIBLE_LIBRARY`
at `sdk/ansible/plugins/modules/` or copy the modules into your playbook's local
`library/` directory to use them.

```yaml
- hosts: localhost
  tasks:
    - name: Create a web server VM
      zyvor_fabric_vm:
        name: web-server
        image: /var/lib/zyvor-fabricd/images/ubuntu-22.04.qcow2
        cpus: 4
        memory: 4096
        disk: 40
        state: started
        api_url: http://localhost:9095
        api_token: "{{ zyvor_fabricd_token }}"

    - name: Stop the VM
      zyvor_fabric_vm:
        name: web-server
        state: stopped
        api_url: http://localhost:9095
        api_token: "{{ zyvor_fabricd_token }}"
```

## Development

This collection targets the Zyvor Fabric REST API (`/api/v1/`). Each module is
self-contained, authenticating with `api_url`/`api_token` and defaulting to
`http://127.0.0.1:9095`, the daemon's default listen address.

### Prerequisites

- Python 3.9+
- Ansible 2.14+
- `requests` Python library
- Running Zyvor Fabric instance

### Testing

```bash
ansible-test units
ansible-test integration
```
