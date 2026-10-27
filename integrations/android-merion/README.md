# Merion for Android

Approve or deny what an agent wants to do, signed on the phone with a P-256 key in the Android Keystore (StrongBox when the
phone has it). It speaks the same `merion-approval-v1` protocol as the iPhone app ([the phone side](../../docs/merion/mobile/README.md)).

| Module | What it is | Needs |
|---|---|---|
| `merionkit` | The protocol: the exact text that is signed, signing and verifying, and an API client (`/v1/inbox`, `/v1/approvals/{id}`). Plain JVM, no Android classes. | A JDK 17 |
| `app` | A small Compose app: the Keystore key (a biometric or screen-lock check on every signature), the text shown before you confirm, Allow and Deny. | The Android SDK |

```bash
cd integrations/android-merion
gradle :merionkit:test        # the core library; needs only a JDK
ANDROID_HOME=… gradle :app:assembleDebug   # the app; needs the Android SDK
```

## What is checked

The six `merionkit` tests were run on 2026-10-27 in a `gradle:8.10-jdk17` container on a Linux host: 6 passed, 0 failed.

`merionkit`'s tests read the runtime's own `docs/merion/mobile/test-vectors.json`, the file the Swift `MerionKit` tests also use:

- the text built for each vector is byte for byte the text the runtime signed;
- the runtime's P-256 signatures verify, as DER and as raw `r||s`;
- a key made here signs and verifies, and a changed decision or a past expiry is refused;
- the client puts the token in the `Authorization` header only, refuses plain `http` except loopback, and sends `device_id` and `signature` together.

## What is not

- **The `app` module has never been built or run.** No Android SDK, emulator or phone was used. The Keystore key, the
  `BiometricPrompt` flow and the screen are written to the documented APIs and nothing more is claimed.
- Push (FCM) needs a Firebase project and a relay ([push relays](../../docs/merion/mobile/README.md)); today the list is pulled with Refresh.
- Enrolment is manual: the app shows its public key and an operator enrols it (`POST /v1/users/<you>/devices`).
- Not built: chat, goals, memory and the Done list that the iPhone app has. No signing key or Play listing exists.
- A Keystore key made with `setInvalidatedByBiometricEnrollment(true)` stops working when a fingerprint is added; enrol the new key.
