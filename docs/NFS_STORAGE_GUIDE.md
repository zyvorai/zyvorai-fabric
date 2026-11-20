# NFS Storage Guide

Store VM disk images and templates on remote NFS servers for centralized storage, simplified backups, and shared access across multiple Zyvor Fabric nodes.

---

## Prerequisites

### System Requirements

- NFS client utilities installed:
  ```bash
  # Fedora/RHEL
  sudo dnf install nfs-utils

  # Debian/Ubuntu
  sudo apt install nfs-common
  ```

- Root privileges (required for mount operations)
- Network connectivity to NFS server
- NFS server configured with appropriate exports

### NFS Server Setup

On your NFS server, export a directory for VM storage:

```bash
# /etc/exports
/export/vm-storage  192.168.1.0/24(rw,sync,no_root_squash,no_subtree_check)
```

Apply the exports:
```bash
sudo exportfs -ra
```

---

## Creating NFS Pools

### Via Web UI

1. Navigate to **Storage Pools** page
2. Click **Create Pool**
3. Select **NFS** type
4. Fill in the configuration:
   - **Pool Name**: Unique identifier (e.g., `nfs-pool-1`)
   - **NFS Server**: Server IP or hostname (e.g., `192.168.1.100`)
   - **Export Path**: NFS export path (e.g., `/export/vm-storage`)
   - **Mount Path**: Local mount point (e.g., `/mnt/nfs-pool`)
   - **NFS Version**: v3, v4, v4.1, or v4.2 (recommended: v4)
   - **Mount Options**: Comma-separated options (default: `rw,hard,intr`)
   - **Auto-start**: Enable to mount on daemon startup
5. Click **Create Pool**

### Via API

```bash
curl -X POST http://localhost:9095/api/storage/pools/nfs \
  -H "Content-Type: application/json" \
  -d '{
    "name": "nfs-pool-1",
    "config": {
      "server": "192.168.1.100",
      "export_path": "/export/vm-storage",
      "mount_path": "/mnt/nfs-pool",
      "mount_options": ["rw", "hard", "intr", "rsize=8192", "wsize=8192"],
      "auto_start": true,
      "nfs_version": "V4"
    }
  }'
```

### Via Configuration File

```toml
# /etc/zyvor-fabricd/zyvor-fabricd.toml

[storage.pools.nfs1]
type = "nfs"
server = "192.168.1.100"
export_path = "/export/vm-storage"
mount_path = "/mnt/nfs-pool"
mount_options = ["rw", "hard", "intr", "rsize=8192", "wsize=8192"]
auto_start = true
nfs_version = "V4"
```

---

## NFS Versions

### NFSv3
- **Use case**: Legacy compatibility, older systems
- **Features**: Simpler protocol, stateless
- **Performance**: Good for large files
- **Recommended**: No (use NFSv4 instead)

### NFSv4
- **Use case**: Modern deployments, recommended default
- **Features**: Stateful, better security, single port
- **Performance**: Excellent
- **Recommended**: Yes

### NFSv4.1
- **Use case**: Enhanced performance, parallel access
- **Features**: pNFS support, improved caching
- **Performance**: Superior for high-throughput workloads
- **Recommended**: Yes (if server supports)

### NFSv4.2
- **Use case**: Latest features, server-side copy
- **Features**: Server-side copy, sparse files, labeled NFS
- **Performance**: Best
- **Recommended**: Yes (if server supports)

---

## Mount Options

### Recommended Options

```
rw,hard,intr,rsize=8192,wsize=8192
```

### Common Options Explained

- **rw**: Read-write access
- **ro**: Read-only access
- **hard**: Hang client on NFS timeout (recommended for VM storage)
- **soft**: Return error on timeout (NOT recommended for VMs)
- **intr**: Allow interruption of NFS requests
- **rsize=N**: Read buffer size in bytes (8192-65536)
- **wsize=N**: Write buffer size in bytes (8192-65536)
- **tcp**: Use TCP (recommended)
- **udp**: Use UDP (legacy)
- **noatime**: Don't update access times (performance boost)
- **nodiratime**: Don't update directory access times

### Performance Tuning

For high-performance workloads:
```
rw,hard,intr,rsize=65536,wsize=65536,tcp,noatime,nodiratime
```

For reliability over performance:
```
rw,hard,sync,rsize=8192,wsize=8192
```

---

## Managing NFS Pools

### Start/Stop Pool

```bash
# Start pool (mount NFS share)
curl -X POST http://localhost:9095/api/storage/pools/nfs-pool-1/start

# Stop pool (unmount NFS share)
curl -X POST http://localhost:9095/api/storage/pools/nfs-pool-1/stop
```

### Check Health

```bash
curl http://localhost:9095/api/storage/pools/nfs-pool-1/health
```

Response:
```json
{
  "status": "Healthy",
  "server_reachable": true,
  "is_mounted": true,
  "last_check": "2026-02-19T12:34:56Z"
}
```

Health statuses:
- **Healthy**: Server reachable, share mounted
- **ServerUnreachable**: Cannot ping NFS server
- **Unmounted**: Server reachable but share not mounted
- **Degraded**: Mounted but experiencing issues

### Get Statistics

```bash
curl http://localhost:9095/api/storage/pools/nfs-pool-1/stats
```

Response:
```json
{
  "total_kb": 1048576000,
  "used_kb": 524288000,
  "available_kb": 524288000,
  "use_percent": 50,
  "mount_point": "/mnt/nfs-pool"
}
```

### Refresh Statistics

```bash
curl -X POST http://localhost:9095/api/storage/pools/nfs-pool-1/refresh
```

---

## Using NFS Pools for VMs

### Create VM with NFS Storage

```bash
fabricctl create my-vm \
  --image=/mnt/nfs-pool/images/ubuntu-22.04.qcow2 \
  --cpus=4 \
  --memory=4096
```

### Store VM Disk on NFS

```bash
# Create qcow2 image on NFS
qemu-img create -f qcow2 /mnt/nfs-pool/vms/my-vm.qcow2 20G

# Use for VM
fabricctl create my-vm --disk=/mnt/nfs-pool/vms/my-vm.qcow2
```

---

## Troubleshooting

### Mount Fails

**Problem**: NFS mount fails with "No such file or directory"

**Solution**:
```bash
# Verify NFS export exists
showmount -e 192.168.1.100

# Test mount manually
sudo mount -t nfs 192.168.1.100:/export/vm-storage /mnt/test

# Check NFS server logs
journalctl -u nfs-server -f
```

### Server Unreachable

**Problem**: "Server unreachable" error

**Solution**:
```bash
# Test connectivity
ping 192.168.1.100

# Test NFS service
rpcinfo -p 192.168.1.100

# Check firewall
sudo firewall-cmd --list-all
```

### Performance Issues

**Problem**: Slow VM performance on NFS

**Solution**:
```bash
# Increase buffer sizes
mount -o remount,rsize=65536,wsize=65536 /mnt/nfs-pool

# Enable async mode (faster but less safe)
mount -o remount,async /mnt/nfs-pool

# Use NFSv4.1 with larger window size
mount -t nfs4 -o vers=4.1,rsize=1048576,wsize=1048576 \
  192.168.1.100:/export/vm-storage /mnt/nfs-pool
```

### Stale File Handles

**Problem**: "Stale file handle" errors

**Solution**:
```bash
# Force unmount
sudo umount -l /mnt/nfs-pool

# Remount
sudo mount -t nfs 192.168.1.100:/export/vm-storage /mnt/nfs-pool
```

---

## Best Practices

### 1. Use NFSv4 or higher
Always prefer NFSv4+ for better security and performance.

### 2. Use hard mounts for VM storage
Never use `soft` mount option for VM disks - data corruption risk.

### 3. Enable auto-start
Set `auto_start: true` for production pools to ensure availability after reboot.

### 4. Monitor health
Regularly check NFS pool health status and set up alerts.

### 5. Optimize buffer sizes
Tune `rsize` and `wsize` based on network MTU and workload:
- 1 Gbps network: rsize/wsize=8192-32768
- 10 Gbps network: rsize/wsize=32768-65536

### 6. Use dedicated NFS network
Isolate NFS traffic on a dedicated network (VLAN) for better performance and security.

### 7. Regular backups
Even with NFS, implement proper backup strategies for VM data.

### 8. Test failover
Regularly test NFS server failover to ensure high availability.

---

## Advanced Configuration

### High Availability NFS

Use NFS with clustered backend storage (e.g., GlusterFS, Ceph):

```toml
[storage.pools.ha-nfs]
type = "nfs"
server = "nfs-vip.example.com"  # Virtual IP managed by Pacemaker
export_path = "/shared/vms"
mount_options = ["rw", "hard", "intr", "_netdev"]
auto_start = true
```

### Read-only NFS for Templates

```bash
# Create read-only pool for template images
curl -X POST http://localhost:9095/api/storage/pools/nfs \
  -d '{
    "name": "templates-ro",
    "config": {
      "server": "192.168.1.100",
      "export_path": "/export/templates",
      "mount_path": "/mnt/templates",
      "mount_options": ["ro", "hard", "noatime"],
      "auto_start": true,
      "nfs_version": "V4"
    }
  }'
```

### Multiple NFS Pools

Organize storage by purpose:

```bash
# Pool 1: Production VMs (fast storage)
/mnt/nfs-prod → 192.168.1.100:/ssd/vms

# Pool 2: Development VMs (slower storage)
/mnt/nfs-dev → 192.168.1.100:/hdd/vms

# Pool 3: Backups (large capacity)
/mnt/nfs-backups → 192.168.1.200:/backup/vms

# Pool 4: Templates (read-only)
/mnt/nfs-templates → 192.168.1.100:/templates
```

---

## Security Considerations

### 1. Network Security
- Use dedicated VLAN for NFS traffic
- Implement firewall rules to restrict NFS access
- Consider IPsec or VPN for encryption

### 2. Export Security
```bash
# /etc/exports - Secure configuration
/export/vm-storage  192.168.1.0/24(rw,sync,no_root_squash,no_subtree_check,sec=sys)
```

### 3. NFSv4 with Kerberos
For maximum security, use NFSv4 with Kerberos authentication:
```bash
mount -t nfs4 -o sec=krb5p nfs-server:/export /mnt/nfs-pool
```

---

## Performance Benchmarking

Test NFS performance before production use:

```bash
# Sequential write test
dd if=/dev/zero of=/mnt/nfs-pool/test.img bs=1M count=1000 oflag=direct

# Sequential read test
dd if=/mnt/nfs-pool/test.img of=/dev/null bs=1M iflag=direct

# Random I/O test with fio
fio --name=random-rw --ioengine=libaio --rw=randrw --bs=4k \
    --numjobs=4 --size=1G --directory=/mnt/nfs-pool --direct=1
```

---

## References

- NFS documentation: https://nfs.sourceforge.net/
- RHEL NFS guide: https://access.redhat.com/documentation/en-us/red_hat_enterprise_linux/9/html/managing_file_systems/exporting-nfs-shares_managing-file-systems
- NFSv4 RFC: https://datatracker.ietf.org/doc/html/rfc7530
