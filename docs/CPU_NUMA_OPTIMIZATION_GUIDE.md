# CPU Pinning and NUMA Optimization

Optimize VM performance with CPU pinning strategies, NUMA-aware placement, and hugepage memory allocation.

---

## Table of Contents

1. [CPU Topology Detection](#cpu-topology-detection)
2. [CPU Pinning Strategies](#cpu-pinning-strategies)
3. [NUMA Topology](#numa-topology)
4. [Memory and Hugepages](#memory-and-hugepages)
5. [Best Practices](#best-practices)
6. [Performance Tuning](#performance-tuning)
7. [Troubleshooting](#troubleshooting)

---

## CPU Topology Detection

### View System Topology

**Via Web UI:**
1. Navigate to **System Resources** page
2. Select **CPU Topology** tab
3. View detailed CPU layout grouped by socket

**Via API:**
```bash
curl http://localhost:9095/api/system/cpu/topology
```

Response:
```json
{
  "total_cpus": 32,
  "sockets": 2,
  "cores_per_socket": 8,
  "threads_per_core": 2,
  "online_cpus": [0, 1, 2, ..., 31],
  "offline_cpus": [],
  "cpus": [
    {
      "id": 0,
      "socket_id": 0,
      "core_id": 0,
      "thread_id": 0,
      "online": true,
      "numa_node": 0
    },
    ...
  ]
}
```

### Understanding the Topology

- **Sockets**: Physical CPU packages
- **Cores**: Physical cores per socket
- **Threads**: Logical CPUs per core (hyperthreading)
- **NUMA Node**: Memory locality domain

**Example System:**
```
2 Sockets × 8 Cores × 2 Threads = 32 vCPUs

Socket 0: CPUs 0-15 (NUMA node 0)
Socket 1: CPUs 16-31 (NUMA node 1)
```

---

## CPU Pinning Strategies

### 1. Auto Pinning (Default)

Let the system scheduler decide CPU placement.

**When to use:**
- Development environments
- Low-priority workloads
- Oversubscribed systems

**Configuration:**
```json
{
  "cpu_pinning": {
    "type": "Auto"
  }
}
```

### 2. NUMA Node Pinning

Pin all VM vCPUs to CPUs in a single NUMA node.

**When to use:**
- Memory-intensive workloads
- Databases
- In-memory caching systems
- Best memory locality

**Configuration:**
```json
{
  "cpu_pinning": {
    "type": "NumaNode",
    "value": 0
  }
}
```

**Example:**
```bash
# Create VM pinned to NUMA node 0
curl -X POST http://localhost:9095/api/vms \
  -d '{
    "name": "database-vm",
    "cpus": 8,
    "memory_mb": 16384,
    "cpu_pinning": {"type": "NumaNode", "value": 0}
  }'
```

### 3. Socket Pinning

Pin all VM vCPUs to CPUs in a single socket.

**When to use:**
- Multi-socket systems
- Cache-coherent workloads
- Avoiding cross-socket communication

**Configuration:**
```json
{
  "cpu_pinning": {
    "type": "Socket",
    "value": 0
  }
}
```

### 4. Explicit Pinning

Manually map each vCPU to a specific physical CPU.

**When to use:**
- Real-time workloads
- Maximum isolation
- Guaranteed performance
- DPDK/SR-IOV workloads

**Configuration:**
```json
{
  "cpu_pinning": {
    "type": "Explicit",
    "value": [
      {"vcpu_id": 0, "physical_cpu": 0},
      {"vcpu_id": 1, "physical_cpu": 2},
      {"vcpu_id": 2, "physical_cpu": 4},
      {"vcpu_id": 3, "physical_cpu": 6}
    ]
  }
}
```

**Best practice for explicit pinning:**
```
Avoid hyperthreading siblings for latency-sensitive VMs:
- Pin to physical cores only (even CPUs)
- vCPU 0 → CPU 0
- vCPU 1 → CPU 2
- vCPU 2 → CPU 4
- vCPU 3 → CPU 6
```

### Setting CPU Pinning

**Via API:**
```bash
curl -X POST http://localhost:9095/api/vms/my-vm/cpu/pin \
  -H "Content-Type: application/json" \
  -d '{
    "pinning": {
      "type": "NumaNode",
      "value": 0
    }
  }'
```

**Via Web UI:**
1. Go to VM details page
2. Click **Configure** → **CPU Settings**
3. Select pinning strategy
4. Apply changes

---

## NUMA Topology

### View NUMA Topology

**Via Web UI:**
1. Navigate to **System Resources** page
2. Select **NUMA Topology** tab

**Via API:**
```bash
curl http://localhost:9095/api/system/numa/topology
```

Response:
```json
{
  "nodes": [
    {
      "id": 0,
      "cpus": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
      "memory_total_mb": 32768,
      "memory_free_mb": 16384,
      "hugepages_2mb_total": 512,
      "hugepages_2mb_free": 256,
      "hugepages_1gb_total": 8,
      "hugepages_1gb_free": 4
    },
    {
      "id": 1,
      "cpus": [16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31],
      "memory_total_mb": 32768,
      "memory_free_mb": 20480,
      "hugepages_2mb_total": 512,
      "hugepages_2mb_free": 384,
      "hugepages_1gb_total": 8,
      "hugepages_1gb_free": 6
    }
  ],
  "distances": [
    [10, 20],
    [20, 10]
  ]
}
```

### Understanding NUMA Distances

Distance values represent memory access latency:
- **10**: Local node (fastest)
- **20**: Remote node (2x slower)
- **Higher values**: Multi-hop NUMA systems

**Example:**
```
Node 0 → Node 0: Distance 10 (local, fast)
Node 0 → Node 1: Distance 20 (remote, slower)
Node 1 → Node 0: Distance 20 (remote, slower)
Node 1 → Node 1: Distance 10 (local, fast)
```

### Automatic NUMA Placement

Get placement recommendation:
```bash
curl "http://localhost:9095/api/system/numa/placement?memory_mb=16384&cpus=8"
```

Response:
```json
{
  "numa_node": 1,
  "cpu_affinity": [16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31]
}
```

Zyvor Fabric selects the node with:
1. Sufficient free memory
2. Enough available CPUs
3. Maximum available memory (tie-breaker)

---

## Memory and Hugepages

### Why Hugepages?

**Benefits:**
- Reduced TLB misses
- Lower memory overhead
- Better performance for large VMs
- Reduced page table walking

**Supported Sizes:**
- **2MB**: Standard hugepages, good for most workloads
- **1GB**: Large hugepages, best for very large VMs (64GB+)

### Allocate Hugepages

**Via Web UI:**
1. Go to **System Resources** → **Memory & Hugepages**
2. Click **Allocate Hugepages**
3. Select size (2MB or 1GB)
4. Enter count
5. Click **Allocate**

**Via API:**
```bash
# Allocate 512 × 2MB hugepages (1GB total)
curl -X POST http://localhost:9095/api/system/memory/hugepages \
  -H "Content-Type: application/json" \
  -d '{
    "size": "Size2MB",
    "count": 512
  }'

# Allocate 16 × 1GB hugepages (16GB total)
curl -X POST http://localhost:9095/api/system/memory/hugepages \
  -d '{
    "size": "Size1GB",
    "count": 16
  }'
```

**Via Kernel Parameters:**
```bash
# /etc/default/grub
GRUB_CMDLINE_LINUX="hugepagesz=2M hugepages=512 hugepagesz=1G hugepages=16"

# Apply
sudo grub2-mkconfig -o /boot/grub2/grub.cfg
sudo reboot
```

### Create VM with Hugepages

```bash
curl -X POST http://localhost:9095/api/vms \
  -d '{
    "name": "big-vm",
    "cpus": 16,
    "memory_mb": 32768,
    "hugepage_size": "Size2MB",
    "numa_node": 0
  }'
```

---

## Best Practices

### 1. NUMA-Aware VM Sizing

**Rule:** Merion VM within a single NUMA node

**Example:**
```
System: 2 NUMA nodes, 16GB per node

Good:
✓ VM with 8GB memory → Fits in single node
✓ VM with 12GB memory → Fits in single node

Bad:
✗ VM with 24GB memory → Spans multiple nodes
  → Use 2 VMs with 12GB each instead
```

### 2. CPU Isolation

For critical VMs, isolate physical cores:

```bash
# /etc/default/grub
GRUB_CMDLINE_LINUX="isolcpus=8-15,24-31 nohz_full=8-15,24-31 rcu_nocbs=8-15,24-31"

# Dedicate CPUs 8-15 and 24-31 to VMs
# Leave CPUs 0-7 and 16-23 for host OS
```

Then pin VMs to isolated CPUs:
```json
{
  "cpu_pinning": {
    "type": "Explicit",
    "value": [
      {"vcpu_id": 0, "physical_cpu": 8},
      {"vcpu_id": 1, "physical_cpu": 10},
      {"vcpu_id": 2, "physical_cpu": 12},
      {"vcpu_id": 3, "physical_cpu": 14}
    ]
  }
}
```

### 3. Avoid Hyperthreading Siblings

For latency-sensitive workloads, pin to physical cores only:

```
System with SMT (Hyperthreading):
CPUs 0,16 = Core 0
CPUs 1,17 = Core 1
CPUs 2,18 = Core 2
...

Pin to: 0, 2, 4, 6, 8, 10, 12, 14 (physical cores)
Avoid: 16, 18, 20, 22, 24, 26, 28, 30 (hyperthread siblings)
```

### 4. Memory Alignment

Align VM memory size to hugepage boundaries:

```
2MB hugepages:
✓ 2048MB = 1024 pages (aligned)
✓ 4096MB = 2048 pages (aligned)
✗ 3000MB = not aligned → wasted memory

1GB hugepages:
✓ 8GB, 16GB, 32GB, 64GB (aligned)
✗ 10GB, 24GB (not aligned)
```

### 5. Monitor NUMA Statistics

Check for cross-NUMA memory access:

```bash
# Per-node memory stats
numastat

# Per-process NUMA stats
numastat -p $(pidof qemu-system-x86_64)
```

Look for:
- High `numa_foreign`: Memory allocated from wrong node
- High `numa_miss`: Remote memory access
- Goal: Both should be near zero

---

## Performance Tuning

### High-Performance Configuration

For maximum performance (database, real-time):

```json
{
  "name": "high-perf-vm",
  "cpus": 8,
  "memory_mb": 16384,
  "cpu_pinning": {
    "type": "NumaNode",
    "value": 0
  },
  "hugepage_size": "Size1GB",
  "numa_node": 0,
  "cpu_shares": 2048,
  "memory": {
    "balloon": false,
    "swap_enabled": false
  }
}
```

**Configuration explained:**
- Pin to NUMA node 0 for CPU and memory locality
- Use 1GB hugepages for large memory allocation
- High CPU shares (2048 vs default 1024) for priority
- Disable ballooning for consistent performance
- Disable swap for latency-sensitive workloads

### Balanced Configuration

For general production use:

```json
{
  "name": "balanced-vm",
  "cpus": 4,
  "memory_mb": 8192,
  "cpu_pinning": {
    "type": "NumaNode",
    "value": 0
  },
  "hugepage_size": "Size2MB",
  "numa_node": 0
}
```

### Development Configuration

For development/testing:

```json
{
  "name": "dev-vm",
  "cpus": 2,
  "memory_mb": 4096,
  "cpu_pinning": {
    "type": "Auto"
  }
}
```

---

## Troubleshooting

### Problem: Poor VM Performance

**Check:**
1. CPU pinning configuration
2. NUMA placement
3. Memory access pattern

**Solution:**
```bash
# Check NUMA stats
numastat -p $(pidof qemu-system-x86_64)

# Verify CPU pinning
taskset -cp $(pidof qemu-system-x86_64)

# Check if VM memory is on correct NUMA node
numactl --hardware
cat /proc/$(pidof qemu-system-x86_64)/numa_maps
```

### Problem: High NUMA Misses

**Cause:** VM spans multiple NUMA nodes

**Solution:**
```bash
# Resize VM to fit in single node
# Or migrate VM to node with more memory

# Get NUMA topology
curl http://localhost:9095/api/system/numa/topology

# Choose node with enough resources
# Pin VM to that node
```

### Problem: Inconsistent Performance

**Cause:** VM not pinned, scheduler moving it between CPUs

**Solution:**
```bash
# Apply CPU pinning
curl -X POST http://localhost:9095/api/vms/my-vm/cpu/pin \
  -d '{"pinning": {"type": "NumaNode", "value": 0}}'
```

### Problem: Out of Hugepages

**Cause:** Not enough hugepages allocated

**Solution:**
```bash
# Check current allocation
cat /proc/meminfo | grep Huge

# Allocate more
echo 1024 | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages

# Or via API
curl -X POST http://localhost:9095/api/system/memory/hugepages \
  -d '{"size": "Size2MB", "count": 1024}'
```

---

## Advanced Scenarios

### Real-Time VM

```json
{
  "name": "rt-vm",
  "cpus": 4,
  "memory_mb": 8192,
  "cpu_pinning": {
    "type": "Explicit",
    "value": [
      {"vcpu_id": 0, "physical_cpu": 8},
      {"vcpu_id": 1, "physical_cpu": 10},
      {"vcpu_id": 2, "physical_cpu": 12},
      {"vcpu_id": 3, "physical_cpu": 14}
    ]
  },
  "cpu_quota": 400000,
  "cpu_period": 100000,
  "hugepage_size": "Size2MB",
  "numa_node": 0
}
```

With host configuration:
```bash
# Isolate CPUs 8-15
isolcpus=8-15 nohz_full=8-15 rcu_nocbs=8-15
```

### Multi-VM NUMA Layout

**2-node system, 4 VMs:**

```
Node 0 (16 CPUs, 32GB):
├── vm1: 4 vCPUs, 8GB (CPUs 0,2,4,6)
└── vm2: 4 vCPUs, 8GB (CPUs 8,10,12,14)

Node 1 (16 CPUs, 32GB):
├── vm3: 4 vCPUs, 8GB (CPUs 16,18,20,22)
└── vm4: 4 vCPUs, 8GB (CPUs 24,26,28,30)
```

Each VM:
- Pinned to one NUMA node
- Uses non-overlapping physical cores
- Avoids hyperthreading siblings
- Maximum isolation and performance

---

## Monitoring and Validation

### Verify CPU Pinning

```bash
# Get VM process ID
VM_PID=$(pidof qemu-system-x86_64)

# Check CPU affinity
taskset -cp $VM_PID

# Expected output for NUMA node 0 pinning:
# pid 12345's current affinity list: 0-15
```

### Monitor NUMA Performance

```bash
# Install numactl
sudo dnf install numactl

# View NUMA stats
watch -n 1 numastat -p $VM_PID

# Check for:
# - numa_hit: should be high
# - numa_miss: should be low
# - numa_foreign: should be low
```

### Performance Metrics

```bash
# CPU pinning effectiveness
perf stat -p $VM_PID -e migrations

# Memory locality
perf mem record -p $VM_PID
perf mem report
```

---

## References

- Linux NUMA documentation: https://www.kernel.org/doc/html/latest/vm/numa.html
- CPU affinity: https://man7.org/linux/man-pages/man2/sched_setaffinity.2.html
- Hugepages: https://www.kernel.org/doc/html/latest/admin-guide/mm/hugetlbpage.html
- NUMA best practices: https://access.redhat.com/documentation/en-us/red_hat_enterprise_linux/9/html/managing_monitoring_and_updating_the_kernel/optimizing-the-memory-and-cpu-subsystems_managing-monitoring-and-updating-the-kernel
