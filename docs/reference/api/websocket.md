# WebSocket API Reference

Detailed specification for the Zyvor Fabric WebSocket console protocol, which provides interactive terminal access to running VMs.

## Table of Contents

- [Connection URL and Authentication](#connection-url-and-authentication)
- [Permission Requirements](#permission-requirements)
- [Message Format](#message-format)
- [Connection Limits](#connection-limits)
- [Idle Timeout](#idle-timeout)
- [Error Handling](#error-handling)
- [Client Examples](#client-examples)

---

## Connection URL and Authentication

### Endpoint

```
ws://<host>:3000/ws/console/:name?token=<jwt>&cols=<cols>&rows=<rows>
```

For TLS-terminated deployments:

```
wss://<host>/ws/console/:name?token=<jwt>&cols=<cols>&rows=<rows>
```

`cols`/`rows` are optional (default `80`/`24`) and size the PTY once at open time -- there is no live resize once the session is running (see [Protocol Notes](#protocol-notes)).

### Authentication

Authentication is performed via the `token` query parameter, not the `Authorization` header. This is because the WebSocket API in most browsers does not support custom headers during the initial handshake.

The token must be a valid JWT obtained from `POST /api/auth/login`.

```
ws://localhost:3000/ws/console/my-vm?token=eyJhbGciOiJIUzI1NiJ9...&cols=120&rows=40
```

### Validation

The server performs these checks during the WebSocket upgrade handshake:

1. **Connection limit** -- Rejects with `503 Service Unavailable` if the maximum concurrent connection limit is reached.
2. **VM name validation** -- Rejects with `400 Bad Request` if the VM name contains invalid characters.
3. **Authentication configured** -- Rejects with `401 Unauthorized` if JWT authentication is not configured on the server.
4. **Token present** -- Rejects with `401 Unauthorized` if the `token` query parameter is missing.
5. **Token valid** -- Rejects with `401 Unauthorized` if the token is expired, malformed, or revoked.
6. **Permission check** -- Rejects with `403 Forbidden` if the user does not have at least write (`User`) role.
7. **Console reachable** -- Rejects with `502 Bad Gateway` if opening the console against the VM's guest agent fails (guest agent disabled, VM unreachable, VM not found). The console is opened *before* the WebSocket upgrade completes, so this comes back as a normal HTTP error rather than a WebSocket that opens and immediately closes.

---

## Permission Requirements

| Role | Access |
|------|--------|
| Admin | Allowed |
| User | Allowed |
| Viewer | Denied (403 Forbidden) |

Console access requires write permission because it provides interactive shell access to the VM, which can modify its state.

---

## Message Format

### Direction: Client to Server (stdin)

Messages sent from the client to the server are forwarded to the VM's stdin.

| Property | Value |
|----------|-------|
| Message type | Binary |
| Encoding | Raw bytes (typically UTF-8 terminal input) |
| Maximum size | 64 KB per message |

### Direction: Server to Client (stdout)

Messages sent from the server to the client contain output from the VM's stdout.

| Property | Value |
|----------|-------|
| Message type | Binary |
| Encoding | Raw bytes (terminal output, may include ANSI escape sequences) |
| Maximum size | 64 KB per message |

### Protocol Notes

- There is no JSON wrapper or framing protocol. Messages are raw byte streams, making this protocol compatible with any terminal emulator library (xterm.js, hterm, etc.).
- The server opens an interactive shell on the VM over FluxVM's vsock guest agent (a real PTY, with job control -- Ctrl-C, Ctrl-Z work normally) and bridges it bidirectionally with the WebSocket connection. There is no live terminal resize once the session is open -- the PTY is sized once at connect time from the `cols`/`rows` query parameters.
- Close frames are handled normally per the WebSocket specification.

---

## Connection Limits

| Parameter | Value |
|-----------|-------|
| Maximum concurrent WebSocket connections | 50 (global, across all VMs) |
| Maximum message size | 64 KB |
| Rejection status | `503 Service Unavailable` |

The connection counter is atomic and decremented when a connection closes (normally or abnormally). If the server is at capacity, new connection attempts receive an immediate `503` response during the HTTP upgrade handshake, before the WebSocket connection is established.

---

## Idle Timeout

| Parameter | Value |
|-----------|-------|
| Idle timeout | 5 minutes (300 seconds) |

If no messages are sent or received on a WebSocket connection for 5 minutes, the server closes the connection. This prevents abandoned connections from consuming resources.

### Keep-Alive

Clients that need to maintain long-lived connections should ensure periodic activity. Terminal emulators typically generate sufficient traffic through cursor blink updates. For programmatic clients, send a single byte (e.g., a null character or a space) periodically to reset the idle timer.

---

## Error Handling

### Handshake Errors

Errors during the WebSocket upgrade are returned as HTTP responses:

| Status | Cause |
|--------|-------|
| `400 Bad Request` | Invalid VM name |
| `401 Unauthorized` | Missing token, invalid/expired token, auth not configured |
| `403 Forbidden` | User role is Viewer (insufficient permissions) |
| `502 Bad Gateway` | Console open failed against the guest agent (disabled, unreachable, or VM not found) |
| `503 Service Unavailable` | Connection limit reached (50 concurrent) |

### Runtime Errors

After the WebSocket connection is established:

- If the VM process exits, the server closes the WebSocket connection.
- If the client sends a message larger than 64 KB, the connection is closed.
- If the idle timeout is reached, the server sends a close frame and terminates the connection.

---

## Client Examples

### websocat (CLI)

```bash
TOKEN=$(curl -s -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"secret"}' | jq -r '.token')

websocat "ws://localhost:3000/ws/console/my-vm?token=$TOKEN"
```

### JavaScript (Browser with xterm.js)

```javascript
import { Terminal } from 'xterm';

const terminal = new Terminal();
terminal.open(document.getElementById('terminal'));

const token = 'eyJhbGciOiJIUzI1NiJ9...';
const vmName = 'my-vm';
const ws = new WebSocket(
  `ws://localhost:3000/ws/console/${vmName}?token=${token}`
);
ws.binaryType = 'arraybuffer';

ws.onopen = () => {
  console.log('Console connected');
};

ws.onmessage = (event) => {
  const text = new TextDecoder().decode(event.data);
  terminal.write(text);
};

ws.onclose = (event) => {
  terminal.write('\r\n[Connection closed]\r\n');
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

// Forward terminal input to the VM
terminal.onData((data) => {
  if (ws.readyState === WebSocket.OPEN) {
    ws.send(new TextEncoder().encode(data));
  }
});
```

### Python

```python
import asyncio
import websockets
import sys

async def console(uri):
    async with websockets.connect(uri) as ws:
        async def recv_loop():
            async for message in ws:
                sys.stdout.buffer.write(message)
                sys.stdout.buffer.flush()

        async def send_loop():
            loop = asyncio.get_event_loop()
            while True:
                data = await loop.run_in_executor(
                    None, sys.stdin.buffer.read, 1
                )
                if not data:
                    break
                await ws.send(data)

        await asyncio.gather(recv_loop(), send_loop())

token = "eyJhbGciOiJIUzI1NiJ9..."
vm_name = "my-vm"
uri = f"ws://localhost:3000/ws/console/{vm_name}?token={token}"
asyncio.run(console(uri))
```
