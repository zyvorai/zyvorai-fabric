#!/usr/bin/python
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# -*- coding: utf-8 -*-

DOCUMENTATION = r'''
---
module: zyvor_fabric_datacenter
short_description: Manage datacenters and clusters in zyvor-fabricd
description:
    - Create and delete datacenters and clusters managed by zyvor-fabricd.
    - Supports idempotent operations with state-based management.
    - Use C(resource_type) to specify whether you are managing a datacenter
      or a cluster.
version_added: "1.0.0"
options:
    name:
        description: Name of the datacenter or cluster.
        required: true
        type: str
    resource_type:
        description: Type of resource to manage.
        choices: ['datacenter', 'cluster']
        default: datacenter
        type: str
    state:
        description: Desired state of the resource.
        choices: ['present', 'absent']
        default: present
        type: str
    description:
        description: Optional description for the datacenter or cluster.
        type: str
    datacenter_id:
        description:
            - ID of the datacenter the cluster belongs to.
            - Required when resource_type is C(cluster) and state is C(present).
        type: str
    ha_enabled:
        description: Enable HA for the cluster.
        type: bool
        default: true
    drs_enabled:
        description: Enable DRS for the cluster.
        type: bool
        default: true
    drs_mode:
        description: DRS automation mode for the cluster.
        type: str
    evc_mode:
        description: EVC mode for the cluster.
        type: str
    resource_id:
        description:
            - Explicit ID to use when looking up or deleting a resource.
            - If not provided, the module searches by name.
        type: str
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
- name: Create a datacenter
  zyvor_fabric_datacenter:
    name: dc-east
    resource_type: datacenter
    state: present
    description: East coast datacenter

- name: Delete a datacenter by name
  zyvor_fabric_datacenter:
    name: dc-east
    resource_type: datacenter
    state: absent

- name: Create a cluster in a datacenter
  zyvor_fabric_datacenter:
    name: prod-cluster
    resource_type: cluster
    state: present
    datacenter_id: "{{ dc_id }}"
    ha_enabled: true
    drs_enabled: true

- name: Delete a cluster by ID
  zyvor_fabric_datacenter:
    name: prod-cluster
    resource_type: cluster
    state: absent
    resource_id: "{{ cluster_id }}"
'''

RETURN = r'''
resource:
    description: The datacenter or cluster object returned by the API.
    type: dict
    returned: when state is present
    sample:
        id: "abc-123"
        name: dc-east
        status: active
resource_id:
    description: The ID of the created or found resource.
    type: str
    returned: when state is present
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
    """Minimal API wrapper for zyvor-fabricd datacenter/cluster endpoints."""

    def __init__(self, base_url, token=None):
        self.base_url = base_url.rstrip("/")
        self.headers = {"Content-Type": "application/json"}
        if token:
            self.headers["Authorization"] = f"Bearer {token}"

    def _url(self, path):
        return f"{self.base_url}/api{path}"

    # -- Datacenter operations --

    def list_datacenters(self):
        resp = requests.get(
            self._url("/datacenters"), headers=self.headers
        )
        resp.raise_for_status()
        return resp.json()

    def find_datacenter_by_name(self, name):
        """Find a datacenter by name. Returns the first match or None."""
        dcs = self.list_datacenters()
        for dc in dcs:
            if dc.get("name") == name:
                return dc
        return None

    def get_datacenter(self, dc_id):
        resp = requests.get(
            self._url(f"/datacenters/{dc_id}"), headers=self.headers
        )
        if resp.status_code == 404:
            return None
        resp.raise_for_status()
        return resp.json()

    def create_datacenter(self, name, description=None):
        data = {"name": name}
        if description:
            data["description"] = description
        resp = requests.post(
            self._url("/datacenters"), headers=self.headers, json=data
        )
        resp.raise_for_status()
        return resp.json()

    def delete_datacenter(self, dc_id):
        resp = requests.delete(
            self._url(f"/datacenters/{dc_id}"), headers=self.headers
        )
        resp.raise_for_status()

    # -- Cluster operations --

    def list_clusters(self):
        resp = requests.get(
            self._url("/clusters"), headers=self.headers
        )
        resp.raise_for_status()
        return resp.json()

    def find_cluster_by_name(self, name):
        """Find a cluster by name. Returns the first match or None."""
        clusters = self.list_clusters()
        for c in clusters:
            if c.get("name") == name:
                return c
        return None

    def get_cluster(self, cluster_id):
        resp = requests.get(
            self._url(f"/clusters/{cluster_id}"), headers=self.headers
        )
        if resp.status_code == 404:
            return None
        resp.raise_for_status()
        return resp.json()

    def create_cluster(
        self,
        name,
        datacenter_id,
        ha_enabled=True,
        drs_enabled=True,
        description=None,
        drs_mode=None,
        evc_mode=None,
    ):
        data = {
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
        resp = requests.post(
            self._url("/clusters"), headers=self.headers, json=data
        )
        resp.raise_for_status()
        return resp.json()

    def delete_cluster(self, cluster_id):
        resp = requests.delete(
            self._url(f"/clusters/{cluster_id}"), headers=self.headers
        )
        resp.raise_for_status()


def run_module():
    module_args = dict(
        name=dict(type='str', required=True),
        resource_type=dict(
            type='str',
            default='datacenter',
            choices=['datacenter', 'cluster'],
        ),
        state=dict(
            type='str',
            default='present',
            choices=['present', 'absent'],
        ),
        description=dict(type='str', default=None),
        datacenter_id=dict(type='str', default=None),
        ha_enabled=dict(type='bool', default=True),
        drs_enabled=dict(type='bool', default=True),
        drs_mode=dict(type='str', default=None),
        evc_mode=dict(type='str', default=None),
        resource_id=dict(type='str', default=None),
        api_url=dict(type='str', default='http://127.0.0.1:9095'),
        api_token=dict(type='str', no_log=True, default=None),
    )

    result = dict(changed=False, resource=dict(), resource_id='')

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
    resource_type = module.params['resource_type']
    state = module.params['state']
    description = module.params['description']
    datacenter_id = module.params['datacenter_id']
    ha_enabled = module.params['ha_enabled']
    drs_enabled = module.params['drs_enabled']
    drs_mode = module.params['drs_mode']
    evc_mode = module.params['evc_mode']
    resource_id = module.params['resource_id']
    api_url = module.params['api_url']
    api_token = module.params['api_token']

    api = ZyvorFabricdAPI(api_url, api_token)

    try:
        if resource_type == 'datacenter':
            _manage_datacenter(
                module, api, result, name, state, description, resource_id
            )
        elif resource_type == 'cluster':
            _manage_cluster(
                module,
                api,
                result,
                name,
                state,
                datacenter_id,
                ha_enabled,
                drs_enabled,
                description,
                drs_mode,
                evc_mode,
                resource_id,
            )
    except requests.exceptions.ConnectionError:
        module.fail_json(
            msg=f"Cannot connect to zyvor-fabricd at {api_url}. "
            "Is the daemon running?",
            exception=traceback.format_exc(),
        )
        return
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

    module.exit_json(**result)


def _manage_datacenter(module, api, result, name, state, description, resource_id):
    """Handle datacenter state management."""
    # Find existing datacenter
    existing = None
    if resource_id:
        existing = api.get_datacenter(resource_id)
    else:
        existing = api.find_datacenter_by_name(name)

    if state == 'present':
        if existing is None:
            # Create the datacenter
            if module.check_mode:
                result['changed'] = True
                return
            dc = api.create_datacenter(name=name, description=description)
            result['changed'] = True
            result['resource'] = dc
            result['resource_id'] = dc.get('id', '')
        else:
            # Datacenter already exists
            result['resource'] = existing
            result['resource_id'] = existing.get('id', '')

    elif state == 'absent':
        if existing is not None:
            if module.check_mode:
                result['changed'] = True
                return
            dc_id = existing.get('id', resource_id)
            api.delete_datacenter(dc_id)
            result['changed'] = True
        # If it doesn't exist, nothing to do


def _manage_cluster(
    module,
    api,
    result,
    name,
    state,
    datacenter_id,
    ha_enabled,
    drs_enabled,
    description,
    drs_mode,
    evc_mode,
    resource_id,
):
    """Handle cluster state management."""
    # Find existing cluster
    existing = None
    if resource_id:
        existing = api.get_cluster(resource_id)
    else:
        existing = api.find_cluster_by_name(name)

    if state == 'present':
        if existing is None:
            # Validate required parameters for creation
            if not datacenter_id:
                module.fail_json(
                    msg="'datacenter_id' is required when creating a new "
                    "cluster (state=present and cluster does not exist)"
                )
                return
            if module.check_mode:
                result['changed'] = True
                return
            cluster = api.create_cluster(
                name=name,
                datacenter_id=datacenter_id,
                ha_enabled=ha_enabled,
                drs_enabled=drs_enabled,
                description=description,
                drs_mode=drs_mode,
                evc_mode=evc_mode,
            )
            result['changed'] = True
            result['resource'] = cluster
            result['resource_id'] = cluster.get('id', '')
        else:
            # Cluster already exists
            result['resource'] = existing
            result['resource_id'] = existing.get('id', '')

    elif state == 'absent':
        if existing is not None:
            if module.check_mode:
                result['changed'] = True
                return
            cluster_id = existing.get('id', resource_id)
            api.delete_cluster(cluster_id)
            result['changed'] = True
        # If it doesn't exist, nothing to do


def main():
    run_module()


if __name__ == '__main__':
    main()
