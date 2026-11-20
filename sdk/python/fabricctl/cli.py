# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""fabricctl CLI - command-line interface for zyvor-fabricd."""

import argparse
import json
import sys

from .client import ZyvorFabricClient


def _print_result(result, output_format="json"):
    """Print a result in the requested format."""
    if result is None:
        return
    if output_format == "json":
        print(json.dumps(result, indent=2, default=str))
    elif output_format == "table":
        if isinstance(result, list):
            if not result:
                print("(no results)")
                return
            headers = list(result[0].keys())
            widths = [len(h) for h in headers]
            for row in result:
                for i, h in enumerate(headers):
                    val = str(row.get(h, ""))
                    widths[i] = max(widths[i], len(val))
            header_line = "  ".join(
                h.ljust(widths[i]) for i, h in enumerate(headers)
            )
            print(header_line)
            print("  ".join("-" * w for w in widths))
            for row in result:
                line = "  ".join(
                    str(row.get(h, "")).ljust(widths[i])
                    for i, h in enumerate(headers)
                )
                print(line)
        elif isinstance(result, dict):
            max_key_len = max(len(k) for k in result.keys()) if result else 0
            for k, v in result.items():
                print(f"{k.ljust(max_key_len)}  {v}")
        else:
            print(result)


def main():
    parser = argparse.ArgumentParser(
        description="fabricctl - CLI for zyvor-fabricd"
    )
    parser.add_argument(
        "--url",
        default="http://127.0.0.1:8080",
        help="zyvor-fabricd API URL (default: http://127.0.0.1:8080)",
    )
    parser.add_argument("--token", help="Authentication token")
    parser.add_argument(
        "--output",
        "-o",
        choices=["json", "table"],
        default="json",
        help="Output format (default: json)",
    )

    subparsers = parser.add_subparsers(dest="command", help="Command to run")

    # ---- VM commands ----
    subparsers.add_parser("vm-list", help="List all VMs")

    p = subparsers.add_parser("vm-get", help="Get VM details")
    p.add_argument("name", help="VM name")

    p = subparsers.add_parser("vm-create", help="Create a VM")
    p.add_argument("--name", required=True, help="VM name")
    p.add_argument("--image", required=True, help="Image path")
    p.add_argument("--cpus", type=int, default=2, help="Number of CPUs")
    p.add_argument("--memory", type=int, default=1024, help="Memory in MB")
    p.add_argument("--disk", type=int, default=20, help="Disk in GB")

    p = subparsers.add_parser("vm-delete", help="Delete a VM")
    p.add_argument("name", help="VM name")

    p = subparsers.add_parser("vm-start", help="Start a VM")
    p.add_argument("name", help="VM name")

    p = subparsers.add_parser("vm-stop", help="Stop a VM")
    p.add_argument("name", help="VM name")

    p = subparsers.add_parser("vm-restart", help="Restart a VM")
    p.add_argument("name", help="VM name")

    p = subparsers.add_parser("vm-metrics", help="Get VM metrics")
    p.add_argument("name", help="VM name")

    # ---- Datacenter commands ----
    subparsers.add_parser("dc-list", help="List all datacenters")

    p = subparsers.add_parser("dc-create", help="Create a datacenter")
    p.add_argument("--name", required=True, help="Datacenter name")
    p.add_argument("--description", help="Description")

    p = subparsers.add_parser("dc-get", help="Get datacenter details")
    p.add_argument("id", help="Datacenter ID")

    p = subparsers.add_parser("dc-delete", help="Delete a datacenter")
    p.add_argument("id", help="Datacenter ID")

    p = subparsers.add_parser("dc-summary", help="Get datacenter summary")
    p.add_argument("id", help="Datacenter ID")

    # ---- Cluster commands ----
    subparsers.add_parser("cluster-list", help="List all clusters")

    p = subparsers.add_parser("cluster-create", help="Create a cluster")
    p.add_argument("--name", required=True, help="Cluster name")
    p.add_argument(
        "--datacenter-id", required=True, help="Datacenter ID"
    )
    p.add_argument(
        "--ha-enabled",
        type=bool,
        default=True,
        help="Enable HA (default: True)",
    )
    p.add_argument(
        "--drs-enabled",
        type=bool,
        default=True,
        help="Enable DRS (default: True)",
    )

    p = subparsers.add_parser("cluster-get", help="Get cluster details")
    p.add_argument("id", help="Cluster ID")

    p = subparsers.add_parser("cluster-delete", help="Delete a cluster")
    p.add_argument("id", help="Cluster ID")

    # ---- Host commands ----
    subparsers.add_parser("host-list", help="List all hosts")

    p = subparsers.add_parser("host-register", help="Register a host")
    p.add_argument("--hostname", required=True, help="Host name")
    p.add_argument("--address", required=True, help="Host address")
    p.add_argument("--cluster-id", required=True, help="Cluster ID")
    p.add_argument("--cpus", type=int, required=True, help="Number of CPUs")
    p.add_argument(
        "--memory-mb", type=int, required=True, help="Memory in MB"
    )

    p = subparsers.add_parser("host-get", help="Get host details")
    p.add_argument("id", help="Host ID")

    p = subparsers.add_parser("host-remove", help="Remove a host")
    p.add_argument("id", help="Host ID")

    p = subparsers.add_parser(
        "host-maintenance-enter", help="Enter maintenance mode"
    )
    p.add_argument("id", help="Host ID")

    p = subparsers.add_parser(
        "host-maintenance-exit", help="Exit maintenance mode"
    )
    p.add_argument("id", help="Host ID")

    # ---- Resource Pool commands ----
    subparsers.add_parser("pool-list", help="List resource pools")

    p = subparsers.add_parser("pool-create", help="Create a resource pool")
    p.add_argument("--name", required=True, help="Pool name")
    p.add_argument("--cluster-id", required=True, help="Cluster ID")

    p = subparsers.add_parser("pool-delete", help="Delete a resource pool")
    p.add_argument("id", help="Pool ID")

    # ---- Storage commands ----
    subparsers.add_parser("storage-list", help="List storage pools")

    # ---- Backup commands ----
    subparsers.add_parser("backup-list", help="List backups")

    p = subparsers.add_parser("backup-create", help="Create a backup")
    p.add_argument("--data", required=True, help="Backup config as JSON")

    # ---- Settings commands ----
    subparsers.add_parser("settings-get", help="Get settings")

    # ---- Health / status ----
    subparsers.add_parser("health", help="Check daemon health")

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        sys.exit(1)

    client = ZyvorFabricClient(args.url, args.token)
    fmt = args.output

    try:
        result = _dispatch(client, args)
        _print_result(result, fmt)
    except Exception as e:
        error_data = {"error": str(e)}
        if hasattr(e, "response") and e.response is not None:
            error_data["status_code"] = e.response.status_code
            try:
                error_data["detail"] = e.response.json()
            except Exception:
                error_data["detail"] = e.response.text
        print(json.dumps(error_data, indent=2), file=sys.stderr)
        sys.exit(1)


def _dispatch(client: ZyvorFabricClient, args) -> object:
    """Dispatch CLI command to the appropriate client method."""
    cmd = args.command

    # VM commands
    if cmd == "vm-list":
        return client.list_vms()
    elif cmd == "vm-get":
        return client.get_vm(args.name)
    elif cmd == "vm-create":
        return client.create_vm(
            name=args.name,
            image=args.image,
            cpus=args.cpus,
            memory=args.memory,
            disk=args.disk,
        )
    elif cmd == "vm-delete":
        client.delete_vm(args.name)
        return {"status": "deleted", "name": args.name}
    elif cmd == "vm-start":
        client.start_vm(args.name)
        return {"status": "started", "name": args.name}
    elif cmd == "vm-stop":
        client.stop_vm(args.name)
        return {"status": "stopped", "name": args.name}
    elif cmd == "vm-restart":
        client.restart_vm(args.name)
        return {"status": "restarted", "name": args.name}
    elif cmd == "vm-metrics":
        return client.get_vm_metrics(args.name)

    # Datacenter commands
    elif cmd == "dc-list":
        return client.list_datacenters()
    elif cmd == "dc-create":
        return client.create_datacenter(
            name=args.name,
            description=getattr(args, "description", None),
        )
    elif cmd == "dc-get":
        return client.get_datacenter(args.id)
    elif cmd == "dc-delete":
        client.delete_datacenter(args.id)
        return {"status": "deleted", "id": args.id}
    elif cmd == "dc-summary":
        return client.get_datacenter_summary(args.id)

    # Cluster commands
    elif cmd == "cluster-list":
        return client.list_clusters()
    elif cmd == "cluster-create":
        return client.create_cluster(
            name=args.name,
            datacenter_id=args.datacenter_id,
            ha_enabled=args.ha_enabled,
            drs_enabled=args.drs_enabled,
        )
    elif cmd == "cluster-get":
        return client.get_cluster(args.id)
    elif cmd == "cluster-delete":
        client.delete_cluster(args.id)
        return {"status": "deleted", "id": args.id}

    # Host commands
    elif cmd == "host-list":
        return client.list_hosts()
    elif cmd == "host-register":
        return client.register_host(
            hostname=args.hostname,
            address=args.address,
            cluster_id=args.cluster_id,
            cpus=args.cpus,
            memory_mb=args.memory_mb,
        )
    elif cmd == "host-get":
        return client.get_host(args.id)
    elif cmd == "host-remove":
        client.remove_host(args.id)
        return {"status": "removed", "id": args.id}
    elif cmd == "host-maintenance-enter":
        client.host_enter_maintenance(args.id)
        return {"status": "maintenance", "id": args.id}
    elif cmd == "host-maintenance-exit":
        client.host_exit_maintenance(args.id)
        return {"status": "connected", "id": args.id}

    # Resource Pool commands
    elif cmd == "pool-list":
        return client.list_resource_pools()
    elif cmd == "pool-create":
        return client.create_resource_pool(
            name=args.name, cluster_id=args.cluster_id
        )
    elif cmd == "pool-delete":
        client.delete_resource_pool(args.id)
        return {"status": "deleted", "id": args.id}

    # Storage commands
    elif cmd == "storage-list":
        return client.list_storage_pools()

    # Backup commands
    elif cmd == "backup-list":
        return client.list_backups()
    elif cmd == "backup-create":
        data = json.loads(args.data)
        return client.create_backup(data)

    # Settings commands
    elif cmd == "settings-get":
        return client.get_settings()

    # Health
    elif cmd == "health":
        return {"status": client.health()}

    else:
        return {"error": f"Unknown command: {cmd}"}


if __name__ == "__main__":
    main()
