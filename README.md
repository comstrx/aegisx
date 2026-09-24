# ✨ AegisX

<div align="center">
   <br/>
   <img height="180" src="https://github.com/user-attachments/assets/428655be-128a-4c24-b5a2-236a07ee6969"/>
   <br/>
   <br/>
</div>

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![CI](https://github.com/comstrx/aegisx/actions/workflows/ci.yaml/badge.svg?branch=main)](https://github.com/comstrx/aegisx/actions/workflows/ci.yaml)
[![Release](https://img.shields.io/github/v/release/comstrx/aegisx?sort=semver)](https://github.com/comstrx/aegisx/releases/latest)

## Overview

`AegisX` is an intelligent API security, observability, threat detection, and automated response platform.

It operates between clients and upstream APIs to observe traffic, discover endpoints, measure performance, detect suspicious behavior, correlate security events, calculate risk, and trigger automated responses.

```text
Client
  ↓
AegisX
  ↓
Upstream API
```

AegisX combines API observability and security intelligence into a single runtime.

## Core Capabilities

### API Observability

Monitor API behavior in real time:

* Request volume
* Response latency
* Throughput
* Error rate
* Status codes
* Endpoint health
* Availability
* Performance degradation
* Traffic anomalies

### API Discovery

Build an API inventory directly from observed traffic:

* Endpoint discovery
* Methods and routes
* New endpoints
* Shadow APIs
* Deprecated APIs
* Unused APIs
* Sensitive endpoints

### Security Analysis

Analyze requests, responses, identities, and traffic patterns using multiple detection layers:

* Security rules
* Machine learning
* Behavioral analysis
* Request and response context
* Historical activity
* Authentication anomalies
* Authorization anomalies
* Injection patterns
* Rate abuse
* Suspicious payloads

### Behavioral Intelligence

AegisX builds behavioral context around APIs and identities instead of treating every request as an isolated event.

```text
Normal Journey

login
  ↓
profile
  ↓
orders
```

```text
Suspicious Journey

login
  ↓
admin
  ↓
users/export
  ↓
payments/refund
```

This allows AegisX to detect unusual API sequences and correlate related events into meaningful attack journeys.

### Attack Correlation

Related security events can be grouped into a single attack timeline.

```text
Initial Access
      ↓
Privilege Discovery
      ↓
Sensitive Resource Access
      ↓
Exfiltration Attempt
      ↓
Sensitive Action
```

This provides context beyond individual alerts.

### Risk Engine

AegisX combines multiple signals to calculate explainable risk.

```text
Risk Score
Confidence
Severity
Evidence
Reasons
```

Example:

```text
Risk       92
Confidence 96%
Severity   Critical

+ ML anomaly
+ unusual endpoint sequence
+ abnormal request rate
+ sensitive resource access
```

The machine-learning model is a signal, not the final authority.

### Policy Engine

Responses are controlled by configurable policies instead of hardcoded application logic.

Conceptually:

```yaml
conditions:
  risk:
    gte: 90

  confidence:
    gte: 0.90

actions:
  - alert
  - rate_limit
  - webhook
```

Policies can combine:

* Risk
* Confidence
* Severity
* Endpoint sensitivity
* Identity
* Traffic behavior
* Security findings

### Automated Response

AegisX can trigger external actions through integrations and webhooks.

Possible actions include:

* Alert
* Rate limit
* Block
* Revoke session
* Revoke token
* Disable API key
* Lock account
* Freeze operation
* Create incident
* Execute webhook

The target system remains responsible for implementing application-specific actions.

## Architecture

```text
                         API Traffic
                              │
                              ▼
                     ┌────────────────┐
                     │ Proxy / Sensor │
                     └───────┬────────┘
                             │
                 ┌───────────┴───────────┐
                 │                       │
                 ▼                       ▼
            Fast Path               Event Pipeline
                 │                       │
                 ▼                       ▼
           Forward Request        Telemetry Collector
                                         │
                         ┌───────────────┼───────────────┐
                         ▼               ▼               ▼
                    Discovery       Performance       Security
                         │               │               │
                         └───────────────┼───────────────┘
                                         ▼
                                  Behavior Engine
                                         │
                                         ▼
                                Correlation Engine
                                         │
                                         ▼
                                    Risk Engine
                                         │
                                         ▼
                                   Policy Engine
                                         │
                                         ▼
                               Response / Webhooks
```

Heavy analysis is performed outside the critical request path whenever possible to minimize additional latency.

## Machine Learning

Model development and production inference are separated.

```text
Dataset
   ↓
Preprocessing
   ↓
Feature Engineering
   ↓
Training
   ↓
Evaluation
   ↓
ONNX Export
   ↓
Rust Runtime
```

Python is used for:

* Dataset processing
* Experimentation
* Feature engineering
* Training
* Evaluation
* Model export

Production inference runs directly inside the Rust runtime through ONNX.

Python is therefore not required for inference in production.

## Runtime

The runtime is implemented in Rust and is responsible for:

* Reverse proxying
* Traffic ingestion
* Telemetry collection
* API discovery
* Rule evaluation
* Feature extraction
* ONNX inference
* Behavioral analysis
* Event correlation
* Risk calculation
* Policy execution
* Control APIs
* Dashboard delivery

The runtime is designed around:

* Rust
* Actix Web
* Tokio
* ONNX Runtime

## Dashboard

The web panel provides a central view of the monitored API environment.

It can expose:

* Overview
* API inventory
* Endpoint health
* Latency
* Throughput
* Error rates
* Security findings
* Risk scores
* Attack journeys
* Behavioral anomalies
* Alerts
* Policies
* Webhooks
* Historical reports

The production frontend can be embedded into the Rust application so the platform can be distributed as a unified executable.

## Repository

```text
aegisx/
├── model/
├── panel/
└── server/
```

### `model`

Machine-learning development:

* Datasets
* Preprocessing
* Feature engineering
* Training
* Evaluation
* ONNX export

### `panel`

Web control plane and visualization interface.

### `server`

Rust runtime containing the proxy, telemetry pipeline, security engines, inference runtime, APIs, and embedded panel delivery.

## Build Model

The release pipeline produces the frontend assets and trained model before compiling the final Rust runtime.

```text
Model Training
      │
      └──→ ONNX Model
                │
Panel Build     │
      │         │
      └────┬────┘
           ▼
      Rust Build
           │
           ▼
        AegisX
```

The resulting runtime can contain:

* Proxy
* Security engine
* Observability engine
* ML model
* Control API
* Background workers
* Dashboard assets

External infrastructure such as persistent databases remains independent from the executable.

## Deployment Model

AegisX is designed to support both simple and distributed deployments.

### Unified

```text
AegisX
├── Proxy
├── Analysis
├── API
├── Workers
└── Dashboard
```

Suitable for development, demonstrations, and smaller deployments.

### Distributed

```text
                  Load Balancer
                       │
          ┌────────────┼────────────┐
          ▼            ▼            ▼
       Gateway      Gateway      Gateway
          │            │            │
          └────────────┼────────────┘
                       ▼
                 Event Pipeline
                       │
                       ▼
                 Analysis Plane
                       │
                 ┌─────┴─────┐
                 ▼           ▼
             Storage      Dashboard
```

The same architecture can evolve without changing the core security model.

## Philosophy

AegisX follows one pipeline:

```text
Discover
   ↓
Observe
   ↓
Understand
   ↓
Detect
   ↓
Correlate
   ↓
Score
   ↓
Respond
```

The goal is not to build another API dashboard or signature-only firewall.

The goal is to build an intelligent security layer that understands how APIs behave, how attacks evolve across requests, and how systems should respond.

## Community

* [Issues](https://github.com/comstrx/aegisx/issues)
* [Discussions](https://github.com/comstrx/aegisx/discussions)
* [Contributing](https://github.com/comstrx/aegisx/blob/main/CONTRIBUTING.md)
* [Security](https://github.com/comstrx/aegisx/blob/main/SECURITY.md)
* [Support](https://github.com/comstrx/aegisx/blob/main/SUPPORT.md)

## License

`AegisX` is dual-licensed under either:

* [MIT](https://github.com/comstrx/aegisx/blob/main/LICENSE-MIT)
* [Apache-2.0](https://github.com/comstrx/aegisx/blob/main/LICENSE-APACHE)

at your option.

Unless explicitly stated otherwise, contributions intentionally submitted for inclusion in this project, as defined by the Apache-2.0 license, are dual-licensed under the same terms.
