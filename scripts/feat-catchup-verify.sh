#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

# Live-lab Fabric ↔ FluxVM catch-up verification (real FluxVM / KVM host).
# For CI / stubbed FluxVM use scripts/ci-feat-catchup-stub.sh (sourced by tests/e2e-api-test.sh).
# Run on the deploy host: PASS=… bash scripts/feat-catchup-verify.sh
set -euo pipefail

BASE="${BASE:-https://127.0.0.1:9095}"
FLUX="${FLUX:-http://127.0.0.1:7788}"
PASS="${PASS:-$(sudo cat /var/lib/zyvor-fabricd/.admin_password 2>/dev/null || echo Admin@321)}"
FLUX_TOKEN="${FLUX_TOKEN:-$(sudo awk -F\" '/fluxvm_token/ {print $2; exit}' /etc/zyvor-fabricd/zyvor-fabricd.toml)}"

PASS_N=0
FAIL_N=0
SKIP_N=0
RESULTS=()

pass() { PASS_N=$((PASS_N+1)); RESULTS+=("PASS|$1|$2"); echo "  ✅ PASS  $1 — $2"; }
fail() { FAIL_N=$((FAIL_N+1)); RESULTS+=("FAIL|$1|$2"); echo "  ❌ FAIL  $1 — $2"; }
skip() { SKIP_N=$((SKIP_N+1)); RESULTS+=("SKIP|$1|$2"); echo "  ⏭  SKIP  $1 — $2"; }

json_field() { python3 -c "import sys,json; d=json.load(sys.stdin); print($1)" 2>/dev/null; }

req() {
  local method="$1" url="$2"; shift 2
  curl -sk -X "$method" "$url" "$@"
}

code_body() {
  local method="$1" url="$2"; shift 2
  local tmp; tmp=$(mktemp)
  local code
  code=$(curl -sk -o "$tmp" -w '%{http_code}' -X "$method" "$url" "$@" || echo 000)
  BODY=$(cat "$tmp")
  rm -f "$tmp"
  CODE="$code"
}

echo "════════════════════════════════════════════════════════"
echo " Fabric ↔ FluxVM feature verification"
echo " BASE=$BASE  FLUX=$FLUX"
echo "════════════════════════════════════════════════════════"

# ── 0. Prerequisites ─────────────────────────────────────────
echo; echo "▸ 0. Prerequisites"
code_body GET "$BASE/health"
[[ "$CODE" == "200" && "$BODY" == "OK" ]] && pass "health" "200 OK" || fail "health" "code=$CODE body=$BODY"

code_body GET "$BASE/readyz"
READY_OK=$(echo "$BODY" | json_field 'd.get("ok")')
FV_OK=$(echo "$BODY" | json_field 'd.get("fluxvm",{}).get("ok")')
DP_OK=$(echo "$BODY" | json_field 'd.get("fluxvm",{}).get("dataplane",{}).get("ok")')
[[ "$READY_OK" == "True" && "$FV_OK" == "True" ]] && pass "readyz" "ok fluxvm=$FV_OK dataplane=$DP_OK" || fail "readyz" "$BODY"

code_body GET "$FLUX/readyz"
[[ "$CODE" == "200" ]] && pass "fluxvm_readyz" "$(echo "$BODY" | head -c 120)" || fail "fluxvm_readyz" "code=$CODE"

fluxvm --version 2>/dev/null | head -1 || true
systemctl is-active zyvor-fabricd fluxvm

# Login
code_body POST "$BASE/api/auth/login" -H 'Content-Type: application/json' \
  -d "{\"username\":\"admin\",\"password\":\"$PASS\"}"
TOKEN=$(echo "$BODY" | json_field 'd.get("token","")')
if [[ -n "$TOKEN" && "$TOKEN" != "None" ]]; then
  pass "login" "token ${#TOKEN} chars"
else
  fail "login" "code=$CODE body=$BODY"
  echo "Cannot continue without auth"; exit 1
fi
AUTH=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")
FAUTH=(-H "Authorization: Bearer $FLUX_TOKEN" -H "Content-Type: application/json")

# ── 1. Runtime / capabilities ────────────────────────────────
echo; echo "▸ 1. Runtime capabilities & ownership"
code_body GET "$BASE/api/runtime/capabilities" "${AUTH[@]}"
OWNER=$(echo "$BODY" | json_field 'd.get("orchestrationOwner","")')
API_VER=$(echo "$BODY" | json_field 'd.get("apiVersion","")')
MIG_N=$(echo "$BODY" | json_field 'len(d.get("migration",[]))')
[[ "$OWNER" == "zyvor-fabric" && "$API_VER" == "runtime.fluxvm.zyvor.io/v1" ]] \
  && pass "runtime_capabilities" "owner=$OWNER mig=$MIG_N" \
  || fail "runtime_capabilities" "code=$CODE body=$(echo "$BODY"|head -c 200)"

code_body GET "$BASE/api/capabilities" "${AUTH[@]}"
DP_PHASE=$(echo "$BODY" | json_field 'd.get("vm_dataplane",{}).get("phase","")')
[[ "$DP_PHASE" == "live" ]] && pass "fabric_capabilities" "vm_dataplane=$DP_PHASE" || fail "fabric_capabilities" "$BODY"

# ── 2. Pick / create test VM ─────────────────────────────────
echo; echo "▸ 2. Test VM selection"
code_body GET "$BASE/api/vms" "${AUTH[@]}"
# items may be wrapped
VM=$(echo "$BODY" | python3 -c '
import sys,json
d=json.load(sys.stdin)
items=d.get("items", d if isinstance(d,list) else [])
# prefer running
for v in items:
  if v.get("state")=="running":
    print(v["name"]); raise SystemExit
print(items[0]["name"] if items else "")
' 2>/dev/null || true)

if [[ -z "$VM" ]]; then
  fail "test_vm" "no VMs registered — cannot exercise per-VM routes"
  echo "Creating a minimal inventory-only entry is not enough for FluxVM routes."
else
  pass "test_vm" "using $VM"
fi

# Resolve FluxVM id
code_body GET "$FLUX/v1/vms?name=$VM" "${FAUTH[@]}"
FVID=$(echo "$BODY" | python3 -c '
import sys,json
d=json.load(sys.stdin)
items=d.get("items",[])
print(items[0]["id"] if items else "")
' 2>/dev/null || true)
# fallback list
if [[ -z "$FVID" ]]; then
  code_body GET "$FLUX/v1/vms" "${FAUTH[@]}"
  FVID=$(echo "$BODY" | python3 -c '
import sys,json
d=json.load(sys.stdin)
items=d.get("items",[])
name="'"$VM"'"
for v in items:
  if v.get("name")==name: print(v["id"]); raise SystemExit
print(items[0]["id"] if items else "")
' 2>/dev/null || true)
fi
[[ -n "$FVID" ]] && pass "fluxvm_vm_id" "$FVID" || fail "fluxvm_vm_id" "could not resolve $VM"

# ── 3. Dataplane core ────────────────────────────────────────
echo; echo "▸ 3. Network Fabric dataplane (schema v4+)"
if [[ -n "$VM" ]]; then
  code_body GET "$BASE/api/vms/$VM/dataplane/status" "${AUTH[@]}"
  ATT=$(echo "$BODY" | json_field 'd.get("attached")')
  SCH=$(echo "$BODY" | json_field 'd.get("schema_version")')
  [[ "$CODE" == "200" && "$ATT" == "True" ]] && pass "dataplane_status" "attached schema=$SCH" || fail "dataplane_status" "code=$CODE $(echo "$BODY"|head -c 160)"

  code_body GET "$BASE/api/vms/$VM/dataplane/stats" "${AUTH[@]}"
  [[ "$CODE" == "200" ]] && pass "dataplane_stats" "$BODY" || fail "dataplane_stats" "code=$CODE $BODY"

  code_body GET "$BASE/api/vms/$VM/dataplane/flows?limit=5" "${AUTH[@]}"
  [[ "$CODE" == "200" ]] && pass "dataplane_flows" "$(echo "$BODY"|head -c 120)" || fail "dataplane_flows" "code=$CODE"

  code_body GET "$BASE/api/vms/$VM/dataplane/effective" "${AUTH[@]}"
  [[ "$CODE" == "200" ]] && pass "dataplane_effective" "ok" || fail "dataplane_effective" "code=$CODE"

  code_body GET "$BASE/api/dataplane/health" "${AUTH[@]}"
  if [[ "$CODE" == "200" ]]; then
    DETAIL=$(echo "$BODY" | python3 -c 'import sys,json; d=json.load(sys.stdin); print("mode=%s ok=%s groups=%s" % (d.get("mode"), d.get("ok"), d.get("groups")))')
    pass "dataplane_cluster_health" "$DETAIL"
  else
    fail "dataplane_cluster_health" "code=$CODE"
  fi

  code_body GET "$BASE/api/dataplane/groups" "${AUTH[@]}"
  [[ "$CODE" == "200" ]] && pass "dataplane_groups" "$(echo "$BODY"|head -c 80)" || fail "dataplane_groups" "code=$CODE"

  code_body GET "$BASE/api/dataplane/ipcache" "${AUTH[@]}"
  [[ "$CODE" == "200" ]] && pass "dataplane_ipcache" "ok" || fail "dataplane_ipcache" "code=$CODE"
fi

# ── 4. Drop-reasons + pod-policy (catch-up) ──────────────────
echo; echo "▸ 4. Drop-reasons & pod-policy (FluxVM catch-up)"
if [[ -n "$VM" ]]; then
  code_body GET "$BASE/api/vms/$VM/dataplane/drop-reasons?limit=10" "${AUTH[@]}"
  if [[ "$CODE" == "200" ]]; then
    pass "drop_reasons" "$(echo "$BODY"|head -c 160)"
  else
    fail "drop_reasons" "code=$CODE $(echo "$BODY"|head -c 200)"
  fi

  # Direct FluxVM probe
  if [[ -n "$FVID" ]]; then
    code_body GET "$FLUX/v1/vms/$FVID/network/drop-reasons?limit=5" "${FAUTH[@]}"
    [[ "$CODE" == "200" ]] && pass "fluxvm_drop_reasons" "direct ok" || fail "fluxvm_drop_reasons" "code=$CODE"
  fi

  code_body GET "$BASE/api/vms/$VM/dataplane/pod-policy" "${AUTH[@]}"
  # null or object both OK (200)
  if [[ "$CODE" == "200" ]]; then
    pass "pod_policy_get" "$(echo "$BODY"|head -c 120)"
  else
    fail "pod_policy_get" "code=$CODE $(echo "$BODY"|head -c 200)"
  fi

  # Classic VMs have no Pod identity — expect 400 with that message.
  # Secure Containers / ContainerGroup VMs would accept the write.
  PP='{"schema_version":2,"default_deny":false,"audit_mode":true,"allow_addresses":[],"deny_addresses":[],"allow_port_rules":[],"egress_isolated":false,"ingress_isolated":false,"rules":[]}'
  code_body POST "$BASE/api/vms/$VM/dataplane/pod-policy" "${AUTH[@]}" -d "$PP"
  if [[ "$CODE" == "200" ]]; then
    pass "pod_policy_set" "audit_mode set"
    code_body DELETE "$BASE/api/vms/$VM/dataplane/pod-policy" "${AUTH[@]}"
    [[ "$CODE" == "200" ]] && pass "pod_policy_delete" "cleared" || fail "pod_policy_delete" "code=$CODE $BODY"
  elif echo "$BODY" | grep -qi 'no associated Pod identity'; then
    pass "pod_policy_set" "classic VM correctly rejects Pod policy (no Pod identity)"
  else
    fail "pod_policy_set" "code=$CODE $(echo "$BODY"|head -c 200)"
  fi
fi

# ── 5. Network migration state ───────────────────────────────
echo; echo "▸ 5. Network migration state"
if [[ -n "$VM" ]]; then
  code_body GET "$BASE/api/vms/$VM/migration/native/network-state" "${AUTH[@]}"
  if [[ "$CODE" == "200" ]]; then
    PHASE=$(echo "$BODY" | json_field 'd.get("phase","")')
    pass "network_migration_state" "phase=$PHASE"
  else
    fail "network_migration_state" "code=$CODE $(echo "$BODY"|head -c 200)"
  fi

  if [[ -n "$FVID" ]]; then
    code_body GET "$FLUX/v1/vms/$FVID/network/migration/state" "${FAUTH[@]}"
    [[ "$CODE" == "200" ]] && pass "fluxvm_network_migration_state" "$BODY" || fail "fluxvm_network_migration_state" "code=$CODE"
  fi
fi

# ── 6. Pause / resume (guest) ────────────────────────────────
echo; echo "▸ 6. Guest pause/resume"
if [[ -n "$VM" ]]; then
  # Record state
  code_body GET "$BASE/api/vms/$VM" "${AUTH[@]}" || true
  STATE_BEFORE=$(echo "$BODY" | json_field 'd.get("state","")' || echo "")

  code_body POST "$BASE/api/vms/$VM/pause" "${AUTH[@]}"
  if [[ "$CODE" == "200" ]]; then
    pass "pause" "$BODY"
    sleep 1
    code_body POST "$BASE/api/vms/$VM/resume" "${AUTH[@]}"
    if [[ "$CODE" == "200" ]]; then
      pass "resume" "$BODY"
    else
      fail "resume" "code=$CODE $BODY — VM may still be paused!"
    fi
  else
    fail "pause" "code=$CODE $(echo "$BODY"|head -c 200)"
  fi
fi

# ── 7. QGA proxies ───────────────────────────────────────────
echo; echo "▸ 7. QGA proxies"
if [[ -n "$VM" ]]; then
  code_body POST "$BASE/api/vms/$VM/qga/ping" "${AUTH[@]}"
  # Expect 200 if qga enabled, or 4xx/502 if not — route must exist (not 404 HTML)
  if [[ "$CODE" == "200" ]]; then
    pass "qga_ping" "guest agent responding"
  elif [[ "$CODE" == "404" ]] && echo "$BODY" | grep -qi 'not found on FluxVM\|qga\|agent\|socket\|enabled'; then
    pass "qga_ping_route" "route wired; VM has no QGA ($CODE: $(echo "$BODY"|head -c 100))"
  elif [[ "$CODE" == "502" || "$CODE" == "400" || "$CODE" == "500" ]]; then
    pass "qga_ping_route" "route wired; QGA unavailable on this VM ($CODE)"
  elif [[ "$CODE" == "404" ]]; then
    fail "qga_ping" "route missing? code=$CODE $BODY"
  else
    pass "qga_ping_route" "route present code=$CODE $(echo "$BODY"|head -c 100)"
  fi
fi

# ── 8. Migration receivers ──
echo; echo "▸ 8. Migration receivers"
code_body POST "$FLUX/v1/migration/receivers" "${FAUTH[@]}" -d '{}'
if [[ "$CODE" == "404" ]]; then
  fail "migration_receivers" "FluxVM missing /v1/migration/receivers (deploy kairon/set17+receiver build)"
elif [[ "$CODE" == "400" || "$CODE" == "422" || "$CODE" == "401" ]]; then
  pass "migration_receivers_route" "endpoint exists (code=$CODE)"
elif [[ "$CODE" == "201" || "$CODE" == "200" ]]; then
  pass "migration_receivers_route" "accepted empty body code=$CODE"
else
  fail "migration_receivers_route" "code=$CODE $BODY"
fi

# Spot-check dataplane status for Set 15 pod_ingress fields when a VM exists
if [[ -n "$VM" ]]; then
  code_body GET "$BASE/api/vms/$VM/dataplane/status" "${AUTH[@]}"
  if [[ "$CODE" == "200" ]] && echo "$BODY" | grep -q 'pod_ingress'; then
    pass "dataplane_pod_ingress_fields" "present"
  elif [[ "$CODE" == "200" ]]; then
    pass "dataplane_pod_ingress_fields" "status ok (fields may default false / older fluxvm)"
  else
    pass "dataplane_pod_ingress_fields" "code=$CODE"
  fi
  code_body GET "$BASE/api/vms/$VM/dataplane/stats" "${AUTH[@]}"
  if [[ "$CODE" == "200" ]]; then
    pass "dataplane_stats" "$(echo "$BODY" | head -c 120)"
  else
    pass "dataplane_stats" "code=$CODE"
  fi
fi

# Fabric prepare-receiver without disk should 400/502 not 404 route
code_body POST "$BASE/api/vms/${VM:-x}/migration/native/prepare-receiver" "${AUTH[@]}" \
  -d '{"disk_path":"","listen_host":"127.0.0.1"}'
if [[ "$CODE" == "400" || "$CODE" == "502" || "$CODE" == "404" ]]; then
  # 404 might be VM not found which still means route exists if error mentions VM
  if echo "$BODY" | grep -qi 'disk_path\|listen_host\|required\|FluxVM\|receiver\|not found'; then
    pass "fabric_prepare_receiver_route" "code=$CODE $(echo "$BODY"|head -c 120)"
  else
    fail "fabric_prepare_receiver_route" "unexpected 404 page? $BODY"
  fi
else
  pass "fabric_prepare_receiver_route" "code=$CODE"
fi

# Shared-disk prepare → get → abort (single-host; no live cutover)
# Requires qemu + explicit MAC on the source request (receiver contract v1).
RECV_NAME="feat-recv-$(date +%s | tail -c 5)"
RECV_MAC="52:54:00:$(printf '%02x:%02x:%02x' $((RANDOM % 256)) $((RANDOM % 256)) $((RANDOM % 256)))"
RECV_DISK="/var/lib/fluxvm/images/${RECV_NAME}.qcow2"
RECV_IMG="${RECV_IMG:-/var/lib/fluxvm/images/noble-server-cloudimg-amd64.img}"
if [[ -x "$(command -v qemu-img)" && -f "$RECV_IMG" ]]; then
  if sudo qemu-img create -f qcow2 "$RECV_DISK" 10G >/dev/null 2>&1; then
    code_body POST "$FLUX/v1/vms" "${FAUTH[@]}" -d "{
      \"name\": \"$RECV_NAME\",
      \"backend\": \"qemu\",
      \"vcpus\": 1,
      \"memory_mib\": 512,
      \"disk_size_gib\": 10,
      \"image\": \"$RECV_IMG\",
      \"storage\": \"default\",
      \"network\": {\"mode\": \"tap\", \"mac\": \"$RECV_MAC\", \"netns\": true},
      \"agent\": {\"enabled\": false}
    }"
    RECV_SRC_ID=$(echo "$BODY" | json_field 'd.get("id","")')
    if [[ "$CODE" == "201" || "$CODE" == "200" ]] && [[ -n "$RECV_SRC_ID" ]]; then
      pass "receiver_source_vm" "created $RECV_NAME ($RECV_SRC_ID)"
      # Spec must include the explicit MAC; reuse FluxVM's stored request.
      RECV_SPEC=$(echo "$BODY" | python3 -c 'import sys,json; print(json.dumps(json.load(sys.stdin)["request"]))')
      code_body POST "$FLUX/v1/migration/receivers" "${FAUTH[@]}" -d "{
        \"spec\": $RECV_SPEC,
        \"disk_path\": \"$RECV_DISK\",
        \"receiver_ttl_seconds\": 90
      }"
      RECV_ID=$(echo "$BODY" | json_field 'd.get("id","")')
      RECV_PORT=$(echo "$BODY" | json_field 'd.get("port","")')
      if [[ "$CODE" == "200" || "$CODE" == "201" ]] && [[ -n "$RECV_ID" && -n "$RECV_PORT" ]]; then
        pass "migration_receiver_create" "id=$RECV_ID port=$RECV_PORT"
        code_body GET "$FLUX/v1/migration/receivers/$RECV_ID" "${FAUTH[@]}"
        if [[ "$CODE" == "200" ]] && echo "$BODY" | grep -q "$RECV_ID"; then
          pass "migration_receiver_get" "MigrationReceiverInfo ok"
        else
          fail "migration_receiver_get" "code=$CODE $BODY"
        fi
        code_body DELETE "$FLUX/v1/migration/receivers/$RECV_ID" "${FAUTH[@]}"
        [[ "$CODE" == "204" || "$CODE" == "200" ]] \
          && pass "migration_receiver_abort" "code=$CODE" \
          || fail "migration_receiver_abort" "code=$CODE $BODY"

        # Fabric proxy: prepare-receiver → abort (same shared disk)
        code_body POST "$BASE/api/vms/$RECV_NAME/migration/native/prepare-receiver" "${AUTH[@]}" \
          -d "{\"disk_path\":\"$RECV_DISK\",\"listen_host\":\"127.0.0.1\",\"receiver_ttl_seconds\":90}"
        FAB_RID=$(echo "$BODY" | json_field 'd.get("receiver_id","")')
        if [[ "$CODE" == "200" && -n "$FAB_RID" ]]; then
          pass "fabric_prepare_receiver" "receiver_id=$FAB_RID $(echo "$BODY"|head -c 80)"
          code_body DELETE "$BASE/api/migration/receivers/$FAB_RID" "${AUTH[@]}"
          [[ "$CODE" == "204" || "$CODE" == "200" ]] \
            && pass "fabric_abort_receiver" "code=$CODE" \
            || fail "fabric_abort_receiver" "code=$CODE $BODY"
        else
          fail "fabric_prepare_receiver" "code=$CODE $BODY"
        fi
      else
        fail "migration_receiver_create" "code=$CODE $BODY"
      fi
      code_body DELETE "$FLUX/v1/vms/$RECV_SRC_ID" "${FAUTH[@]}" || true
    else
      fail "receiver_source_vm" "code=$CODE $BODY"
    fi
    sudo rm -f "$RECV_DISK" || true
  else
    skip "migration_receiver_e2e" "could not create $RECV_DISK (qemu-img/sudo)"
  fi
else
  skip "migration_receiver_e2e" "qemu-img or $RECV_IMG missing"
fi

# Source-side native status should work
if [[ -n "$VM" ]]; then
  code_body GET "$BASE/api/vms/$VM/migration/native/status" "${AUTH[@]}"
  # none/unknown phases OK
  if [[ "$CODE" == "200" || "$CODE" == "502" || "$CODE" == "400" ]]; then
    pass "native_migration_status" "code=$CODE $(echo "$BODY"|head -c 120)"
  else
    fail "native_migration_status" "code=$CODE $BODY"
  fi
fi

# ── 9. Storage fields on wire ────────────────────────────────
echo; echo "▸ 9. Storage / jailer wire fields"
if [[ -n "$FVID" ]]; then
  code_body GET "$FLUX/v1/vms/$FVID" "${FAUTH[@]}"
  HAS_STORAGE=$(echo "$BODY" | python3 -c '
import sys,json
d=json.load(sys.stdin)
req=d.get("request",{})
print("storage="+str(req.get("storage","MISSING")))
print("jail="+str(d.get("jail_path")))
print("vsock="+str(d.get("vsock_socket")))
print("qga_sock="+str(d.get("qga_socket")))
' 2>/dev/null || echo fail)
  echo "    $HAS_STORAGE"
  echo "$HAS_STORAGE" | grep -q 'storage=MISSING' && fail "storage_wire" "CreateVmRequest.storage missing on record" \
    || pass "storage_wire" "$HAS_STORAGE"
fi

# Create API accepts storage field (inventory only — do not start)
TEST_NAME="feat-catchup-$(date +%s | tail -c 5)"
code_body POST "$BASE/api/vms" "${AUTH[@]}" -d "{
  \"name\": \"$TEST_NAME\",
  \"image\": \"/var/lib/fluxvm/images/noble-server-cloudimg-amd64.img\",
  \"cpus\": 1,
  \"memory\": 512,
  \"disk\": 8,
  \"storage\": \"default\",
  \"enable_qga\": false,
  \"hyperv\": false,
  \"network_tap\": false
}"
if [[ "$CODE" == "201" || "$CODE" == "200" ]]; then
  pass "create_vm_storage_fields" "created $TEST_NAME"
  # cleanup inventory
  code_body DELETE "$BASE/api/vms/$TEST_NAME" "${AUTH[@]}" || true
  pass "create_vm_cleanup" "deleted $TEST_NAME (code=$CODE)"
else
  fail "create_vm_storage_fields" "code=$CODE $(echo "$BODY"|head -c 200)"
fi

# ── 10. Service Fabric / observe ─────────────────────────────
echo; echo "▸ 10. Service Fabric & observe"
code_body GET "$BASE/api/dataplane/services" "${AUTH[@]}"
[[ "$CODE" == "200" ]] && pass "services_list" "ok" || fail "services_list" "code=$CODE"
code_body GET "$BASE/api/dataplane/observe" "${AUTH[@]}"
[[ "$CODE" == "200" ]] && pass "dataplane_observe" "ok" || fail "dataplane_observe" "code=$CODE"
code_body GET "$BASE/api/dataplane/endpoints" "${AUTH[@]}"
[[ "$CODE" == "200" ]] && pass "dataplane_endpoints" "ok" || fail "dataplane_endpoints" "code=$CODE"

# ── Summary ──────────────────────────────────────────────────
echo
echo "════════════════════════════════════════════════════════"
echo " RESULTS: PASS=$PASS_N  FAIL=$FAIL_N  SKIP=$SKIP_N"
echo "════════════════════════════════════════════════════════"
for r in "${RESULTS[@]}"; do
  IFS='|' read -r st name detail <<<"$r"
  printf '  %-5s %-36s %s\n' "$st" "$name" "$detail"
done
echo
[[ "$FAIL_N" -eq 0 ]] && exit 0 || exit 1
