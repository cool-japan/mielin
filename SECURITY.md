# Security Policy

MielinOS is a microkernel-based operating system for distributed AI agents,
maintained by **COOLJAPAN OU (Team Kitasan)**. We take the security of the
project seriously and appreciate the community's help in identifying and
responsibly disclosing vulnerabilities.

> **Pre-1.0 notice:** MielinOS is currently at **v0.1.0** ("Oligodendrocyte")
> and under active development. APIs, wire protocols, and internal security
> mechanisms may change between minor releases without the stability
> guarantees of a 1.0+ release. There is no long-term support (LTS) line yet
> — see the [Roadmap](./README.md#roadmap) for where hardening work lands.

## Supported Versions

Only the latest `0.1.x` release is currently supported with security fixes.
Because the project has not yet reached a 1.0 release, we do not maintain
parallel security-patch branches across minor versions.

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1.0 | :x:                 |

This table will be expanded once MielinOS reaches 1.0 and a formal
long-term-support policy is established (see the "Saltatory" v1.0 milestone
in [README.md](./README.md#roadmap)).

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub
issues, discussions, or pull requests.**

The preferred and primary channel for reporting a security vulnerability is
**GitHub's private security advisories**, which keep the report confidential
between you and the maintainers until a fix is ready:

**➡ [Report a vulnerability via GitHub Security Advisories](https://github.com/cool-japan/mielin/security/advisories/new)**

You can also review previously published advisories (once any exist) on the
repository's [Security tab](https://github.com/cool-japan/mielin/security).

If you are unable to use GitHub's private advisory form for any reason,
please open a minimal, non-sensitive placeholder issue asking a maintainer
to contact you through a secure channel, and a member of the COOLJAPAN /
Team Kitasan maintainers will follow up.

### What to include in a report

To help us triage and reproduce the issue quickly, please include as much of
the following as you can:

- A clear description of the vulnerability and its potential impact.
- The affected crate(s) and version(s) (e.g. `mielin-kernel 0.1.0`,
  `mielin-mesh-wire 0.1.0`).
- Steps to reproduce, a minimal proof-of-concept, or a failing test case.
- Any relevant configuration (target architecture, feature flags, whether
  the issue requires bare-metal/kernel access, WASM sandbox context,
  network/mesh topology, etc.).
- Your assessment of severity and any suggested mitigation, if you have one.

### What to expect

- **Acknowledgement:** We aim to acknowledge new reports within a few
  business days.
- **Triage:** We will investigate and aim to provide an initial assessment
  (confirmed, needs more information, or not applicable) within
  approximately 10 business days of acknowledgement. Timelines may vary
  depending on complexity and maintainer availability, as MielinOS is
  maintained by a small team.
- **Coordinated disclosure:** We ask that you give us a reasonable
  opportunity to investigate and address a confirmed vulnerability before
  any public disclosure. We will work with you on a mutually agreeable
  disclosure timeline, and will credit reporters (unless you prefer to
  remain anonymous) in the eventual advisory and/or `CHANGELOG.md` entry.
- **Fix and advisory:** Confirmed vulnerabilities will be fixed in a
  patch release and disclosed via a GitHub Security Advisory referencing
  the affected crate(s) and version range.

We do not currently operate a paid bug bounty program.

## Scope

### In scope

Security reports are welcome for any crate in this workspace, including but
not limited to:

- `mielin` — meta crate / public API surface
- `mielin-kernel` — microkernel and capability-based IPC
- `mielin-hal` — hardware abstraction layer
- `mielin-rt` — embedded runtime
- `mielin-mesh/core` (`mielin-mesh-core`) — DHT and mesh routing
- `mielin-mesh/wire` (`mielin-mesh-wire`) — QUIC transport, TLS/mTLS,
  certificate management, post-quantum hybrid key exchange
- `mielin-cells` — agent SDK and lifecycle management
- `mielin-wasm` — WebAssembly agent runtime and sandboxing
- `mielin-tensor` — tensor operations
- `mielin-cli` — command-line interface

Vulnerability classes of particular interest include: capability/sandbox
escapes, WASM isolation bypasses, memory-safety issues in `unsafe` code,
TLS/mTLS misconfiguration or certificate-validation bypasses, cryptographic
implementation flaws, and denial-of-service vectors in the mesh/wire
protocol.

### Out of scope

- Vulnerabilities in third-party dependencies (e.g. crates under the
  `oxicrypto-*`, `oxiquic-*`, `oxihttp-*`, `rustls`, `rcgen`, `x509-parser`,
  or `p256` families, or any other upstream crate). Please report these to
  the upstream project directly; if the issue is exploitable specifically
  through how MielinOS uses that dependency, we still want to hear about it
  here.
- Findings that require physical access to a machine, a compromised
  operating system/host, or a compromised build toolchain.
- Best-practice suggestions, missing defense-in-depth hardening, or
  configuration recommendations that do not demonstrate an actual
  vulnerability (these are welcome as regular GitHub issues instead).
- Denial-of-service reports that rely purely on resource exhaustion from
  an already-authenticated/trusted peer acting within its granted
  capabilities.
- Documented, in-source limitations that are already flagged as
  intentional scaffolding or "not yet implemented" (for example, features
  explicitly marked as future work in `docs/ARCHITECTURE.md` or
  `docs/CERTIFICATES.md`) — these are tracked as development work, not
  security incidents, unless they cause a silent, undocumented security
  guarantee to be broken.

If you are unsure whether something is in scope, please report it anyway —
we would rather triage a false positive than miss a real issue.

## Security Model

This section is a brief, high-level summary for reporters and users. It is
**not** a substitute for the authoritative documents, which describe the
actual implementation in detail:

- [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) — capability-based
  security model, WASM sandbox isolation guarantees, agent identity and
  authentication flow.
- [`docs/CERTIFICATES.md`](./docs/CERTIFICATES.md) — TLS 1.3 / mTLS trust
  model, certificate generation/rotation/pinning, and the pure-Rust crypto
  stack used by the mesh wire protocol.

At a high level, MielinOS's security posture is built around:

- **Capability-based access control.** Agents start with zero capabilities
  and must be explicitly granted permissions (e.g. network, filesystem)
  before they can use them; this follows a zero-trust, least-privilege
  model at the agent boundary.
- **WASM sandbox isolation.** Agent code runs inside a WebAssembly sandbox
  (Wasmtime) with linear-memory isolation, no direct hardware access, and
  configurable resource limits, rather than running natively on the host.
- **TLS 1.3 / mTLS for mesh transport.** The `mielin-mesh-wire` QUIC
  transport uses TLS 1.3 exclusively (no fallback to older TLS versions),
  with optional mutual TLS, CA/chain validation, and certificate pinning
  for node-to-node authentication — see `docs/CERTIFICATES.md` for exact
  guarantees and known limitations (for example, OCSP revocation checking
  is currently scaffolding pending a suitable pure-Rust dependency, and CRL
  checking is the more complete path today).
- **Pure-Rust cryptography.** The mesh wire crypto stack is built on
  pure-Rust crates in the COOLJAPAN ecosystem (`oxicrypto-hash`,
  `oxicrypto-sig`, `oxiquic-crypto`, `oxitls-rcgen`) plus `rustls`, rather
  than `ring` or `aws-lc-rs`, avoiding non-Rust cryptographic dependencies
  in that layer.
- **Post-quantum-ready key exchange.** `mielin-mesh-wire` implements a
  hybrid X25519 + ML-KEM (NIST FIPS 203) key encapsulation mechanism,
  intended to provide forward-looking resistance to future quantum
  adversaries in addition to classical Diffie-Hellman security.

These mechanisms are actively evolving as the project moves toward 1.0.
Some capabilities described in the architecture documentation are marked
as forward-looking ("future") work rather than shipped behavior — always
check the linked documents and the relevant crate's source/tests for the
current state of a specific guarantee before relying on it in a
security-sensitive deployment. **MielinOS has not yet undergone an
independent third-party security audit;** treat it accordingly for
production use ahead of a 1.0 release.

## Security Updates and Disclosure Process

1. A vulnerability is reported privately via a
   [GitHub security advisory](https://github.com/cool-japan/mielin/security/advisories/new).
2. Maintainers acknowledge the report, assign a severity, and begin
   investigation, coordinating with the reporter as needed.
3. A fix is developed in a private fork or branch associated with the
   advisory, so the vulnerability is not disclosed publicly before a patch
   is available.
4. A new patch release is published to [crates.io](https://crates.io/crates/mielin)
   and tagged on GitHub, alongside a `CHANGELOG.md` entry describing the
   fix (without unnecessarily aiding exploitation of unpatched versions).
5. The GitHub security advisory is published, crediting the reporter
   (unless anonymity is requested), including affected version ranges and
   remediation guidance.
6. For vulnerabilities affecting dependencies, we will update the
   dependency and release a new version as soon as reasonably possible
   after an upstream fix is available, consistent with our
   [latest-crates policy](./CONTRIBUTING.md).

Thank you for helping keep MielinOS and its users safe.
