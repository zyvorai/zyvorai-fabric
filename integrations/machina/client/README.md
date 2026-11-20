# machina-fabric

Machina v0.1 CLI — probes Zyvor Fabric (`zyvor-fabricd`) clusters from macOS or Linux.

```bash
mkdir -p ~/.machina
cp ../clusters.example.yaml ~/.machina/clusters.yaml
cargo build --release

export ZYVOR_FABRIC_USER=admin
export ZYVOR_FABRIC_PASSWORD="$(sudo cat /var/lib/zyvor-fabricd/.admin_password)"

./target/release/machina-fabric health
./target/release/machina-fabric vms
./target/release/machina-fabric vms metrics my-vm
./target/release/machina-fabric events
./target/release/machina-fabric watch
./target/release/machina-fabric logs --lines 30
./target/release/machina-fabric logs --vm my-vm --lines 30
```

See [docs/integrations/machina.md](../../../docs/integrations/machina.md).
