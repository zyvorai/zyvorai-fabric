#!/usr/bin/python
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# -*- coding: utf-8 -*-

DOCUMENTATION = r'''
---
module: zyvor_fabric_vm
short_description: Manage VMs in zyvor-fabricd
description:
    - Create, delete, start, and stop virtual machines managed by zyvor-fabricd.
    - Supports idempotent operations with state-based management.
version_added: "1.0.0"
options:
    name:
        description: Name of the VM to manage.
        required: true
        type: str
    state:
        description: Desired state of the VM.
        choices: ['present', 'absent', 'started', 'stopped', 'restarted']
        default: present
        type: str
    image:
        description:
            - Path to the VM image file.
            - Required when state is C(present) and the VM does not already exist.
        type: str
    cpus:
        description: Number of virtual CPUs to allocate.
        type: int
        default: 2
    memory:
        description: Amount of memory in MB to allocate.
        type: int
        default: 1024
    disk:
        description: Disk size in GB.
        type: int
        default: 20
    tags:
        description: List of tags to assign to the VM.
        type: list
        elements: str
    api_url:
        description: URL of the zyvor-fabricd API server.
        type: str
        default: "http://127.0.0.1:9095"
    api_token:
        description: Bearer token for API authentication.
        type: str
        no_log: true
author:
    - zyvor-fabricd contributors
'''

EXAMPLES = r'''
- name: Create a VM
  zyvor_fabric_vm:
    name: web-01
    state: present
    image: /var/lib/zyvor-fabricd/images/ubuntu.img
    cpus: 4
    memory: 4096
    disk: 40

- name: Start a VM
  zyvor_fabric_vm:
    name: web-01
    state: started

- name: Stop a VM
  zyvor_fabric_vm:
    name: web-01
    state: stopped

- name: Restart a VM
  zyvor_fabric_vm:
    name: web-01
    state: restarted

- name: Delete a VM
  zyvor_fabric_vm:
    name: web-01
    state: absent
'''

RETURN = r'''
vm:
    description: The VM object returned by the API.
    type: dict
    returned: when state is present, started, or stopped
    sample:
        name: web-01
        state: running
        cpus: 4
        memory: 4096
        image: /var/lib/zyvor-fabricd/images/ubuntu.img
changed:
    description: Whether the module made any changes.
    type: bool
    returned: always
'''

import json
import traceback

try:
    import requests
    HAS_REQUESTS = True
except ImportError:
    HAS_REQUESTS = False

from ansible.module_utils.basic import AnsibleModule


class ZyvorFabricdAPI:
    """Minimal API wrapper for the zyvor-fabricd REST API."""

    def __init__(self, base_url, token=None):
        self.base_url = base_url.rstrip("/")
        self.headers = {"Content-Type": "application/json"}
        if token:
            self.headers["Authorization"] = f"Bearer {token}"

    def _url(self, path):
        return f"{self.base_url}/api{path}"

    def get_vm(self, name):
        """Get a VM by name. Returns the VM dict or None if not found."""
        resp = requests.get(self._url(f"/vms/{name}"), headers=self.headers)
        if resp.status_code == 404:
            return None
        resp.raise_for_status()
        return resp.json()

    def create_vm(self, name, image, cpus, memory, disk=20, tags=None):
        """Create a new VM."""
        data = {
            "name": name,
            "image": image,
            "cpus": cpus,
            "memory": memory,
        }
        if disk != 20:
            data["disk"] = disk
        if tags:
            data["tags"] = tags
        resp = requests.post(
            self._url("/vms"), headers=self.headers, json=data
        )
        resp.raise_for_status()
        return resp.json()

    def delete_vm(self, name):
        """Delete a VM by name."""
        resp = requests.delete(
            self._url(f"/vms/{name}"), headers=self.headers
        )
        resp.raise_for_status()

    def start_vm(self, name):
        """Start a VM."""
        resp = requests.post(
            self._url(f"/vms/{name}/start"), headers=self.headers
        )
        resp.raise_for_status()

    def stop_vm(self, name):
        """Stop a VM."""
        resp = requests.post(
            self._url(f"/vms/{name}/stop"), headers=self.headers
        )
        resp.raise_for_status()

    def restart_vm(self, name):
        """Restart a VM."""
        resp = requests.post(
            self._url(f"/vms/{name}/restart"), headers=self.headers
        )
        resp.raise_for_status()


def run_module():
    module_args = dict(
        name=dict(type='str', required=True),
        state=dict(
            type='str',
            default='present',
            choices=['present', 'absent', 'started', 'stopped', 'restarted'],
        ),
        image=dict(type='str', default=None),
        cpus=dict(type='int', default=2),
        memory=dict(type='int', default=1024),
        disk=dict(type='int', default=20),
        tags=dict(type='list', elements='str', default=None),
        api_url=dict(type='str', default='http://127.0.0.1:9095'),
        api_token=dict(type='str', no_log=True, default=None),
    )

    result = dict(changed=False, vm=dict())

    module = AnsibleModule(
        argument_spec=module_args,
        supports_check_mode=True,
    )

    if not HAS_REQUESTS:
        module.fail_json(
            msg="The 'requests' Python library is required. "
            "Install it with: pip install requests"
        )

    name = module.params['name']
    state = module.params['state']
    image = module.params['image']
    cpus = module.params['cpus']
    memory = module.params['memory']
    disk = module.params['disk']
    tags = module.params['tags']
    api_url = module.params['api_url']
    api_token = module.params['api_token']

    api = ZyvorFabricdAPI(api_url, api_token)

    try:
        existing_vm = api.get_vm(name)
    except requests.exceptions.ConnectionError:
        module.fail_json(
            msg=f"Cannot connect to zyvor-fabricd at {api_url}. "
            "Is the daemon running?"
        )
        return
    except requests.exceptions.HTTPError as e:
        module.fail_json(
            msg=f"Failed to query VM '{name}': {str(e)}",
            exception=traceback.format_exc(),
        )
        return

    try:
        if state == 'present':
            if existing_vm is None:
                # VM does not exist, create it
                if image is None:
                    module.fail_json(
                        msg="'image' is required when creating a new VM "
                        "(state=present and VM does not exist)"
                    )
                    return
                if module.check_mode:
                    result['changed'] = True
                    module.exit_json(**result)
                    return

                vm = api.create_vm(
                    name=name,
                    image=image,
                    cpus=cpus,
                    memory=memory,
                    disk=disk,
                    tags=tags,
                )
                result['changed'] = True
                result['vm'] = vm
            else:
                # VM already exists, no change needed
                result['vm'] = existing_vm

        elif state == 'absent':
            if existing_vm is not None:
                if module.check_mode:
                    result['changed'] = True
                    module.exit_json(**result)
                    return
                api.delete_vm(name)
                result['changed'] = True
            # If VM doesn't exist, nothing to do

        elif state == 'started':
            if existing_vm is None:
                # Auto-create and start if image is provided
                if image is not None:
                    if module.check_mode:
                        result['changed'] = True
                        module.exit_json(**result)
                        return
                    vm = api.create_vm(
                        name=name,
                        image=image,
                        cpus=cpus,
                        memory=memory,
                        disk=disk,
                        tags=tags,
                    )
                    result['changed'] = True
                    # Check if already running after creation
                    if vm.get('state') != 'running':
                        api.start_vm(name)
                    result['vm'] = api.get_vm(name) or vm
                else:
                    module.fail_json(
                        msg=f"VM '{name}' does not exist. Provide 'image' "
                        "to auto-create it."
                    )
                    return
            else:
                vm_state = existing_vm.get('state', '')
                if vm_state != 'running':
                    if module.check_mode:
                        result['changed'] = True
                        module.exit_json(**result)
                        return
                    api.start_vm(name)
                    result['changed'] = True
                    result['vm'] = api.get_vm(name) or existing_vm
                else:
                    # Already running
                    result['vm'] = existing_vm

        elif state == 'stopped':
            if existing_vm is None:
                module.fail_json(
                    msg=f"VM '{name}' does not exist. Cannot stop."
                )
                return
            else:
                vm_state = existing_vm.get('state', '')
                if vm_state != 'stopped':
                    if module.check_mode:
                        result['changed'] = True
                        module.exit_json(**result)
                        return
                    api.stop_vm(name)
                    result['changed'] = True
                    result['vm'] = api.get_vm(name) or existing_vm
                else:
                    # Already stopped
                    result['vm'] = existing_vm

        elif state == 'restarted':
            if existing_vm is None:
                module.fail_json(
                    msg=f"VM '{name}' does not exist. Cannot restart."
                )
                return
            else:
                if module.check_mode:
                    result['changed'] = True
                    module.exit_json(**result)
                    return
                api.restart_vm(name)
                result['changed'] = True
                result['vm'] = api.get_vm(name) or existing_vm

    except requests.exceptions.HTTPError as e:
        error_detail = str(e)
        if hasattr(e, 'response') and e.response is not None:
            try:
                error_detail = e.response.json().get('error', str(e))
            except (ValueError, AttributeError):
                error_detail = e.response.text or str(e)
        module.fail_json(
            msg=f"zyvor-fabricd API error: {error_detail}",
            exception=traceback.format_exc(),
        )
        return
    except requests.exceptions.ConnectionError:
        module.fail_json(
            msg=f"Lost connection to zyvor-fabricd at {api_url}.",
            exception=traceback.format_exc(),
        )
        return

    module.exit_json(**result)


def main():
    run_module()


if __name__ == '__main__':
    main()
