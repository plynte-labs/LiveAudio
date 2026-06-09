# Local Observability Specification

## Purpose

Define how LiveAudio SHALL collect and expose local-only runtime and test-health diagnostics without introducing remote telemetry or sensitive-data leakage.

## Requirements

### Requirement: Local-Only Diagnostics Boundary

The system MUST keep observability data local by default and MUST NOT transmit diagnostics to external services unless a future explicit opt-in capability is separately specified.

#### Scenario: Local diagnostics enabled
- GIVEN diagnostics are enabled locally
- WHEN the app records runtime or test-health signals
- THEN the data remains on the local machine
- AND no remote endpoint is contacted

#### Scenario: Sanitized troubleshooting export
- GIVEN a user exports a diagnostics report
- WHEN the report is generated
- THEN secrets, raw audio, raw transcripts, personal absolute paths, and private URLs are excluded or redacted

### Requirement: Runtime Pipeline Health Signals

The system MUST surface bounded runtime health signals for the audio, ASR, and WebSocket pipeline.

#### Scenario: Runtime snapshot requested
- GIVEN LiveAudio is running
- WHEN a diagnostics snapshot is collected
- THEN it includes queue depth, backlog state, connected WebSocket clients, active worker/process state, and recent stage timings

#### Scenario: Slow stage detected
- GIVEN a pipeline stage exceeds its expected duration threshold
- WHEN diagnostics are captured
- THEN the snapshot marks the degraded stage and its measured duration

### Requirement: Test and Teardown Health Signals

The system MUST surface health signals that help identify test-suite exit problems and resource leaks.

#### Scenario: Test run completes with lingering resources
- GIVEN a test file finishes its assertions
- WHEN teardown diagnostics run
- THEN lingering queues, managers, processes, or threads are reported with enough context to identify the source area

#### Scenario: Test timing summary requested
- GIVEN a diagnostics-enabled test session
- WHEN the run ends
- THEN per-file duration and teardown anomalies are available in a local summary

### Requirement: Bounded Overhead and Control Levels

The system MUST provide explicit diagnostics control levels so maintainers can trade detail for overhead.

#### Scenario: Minimal diagnostics mode
- GIVEN diagnostics are enabled in minimal mode
- WHEN LiveAudio runs normally
- THEN only bounded counters, timings, and lifecycle summaries are collected
- AND hot-path behavior is not materially altered

#### Scenario: Deep diagnostics mode
- GIVEN a maintainer enables a deeper diagnostic level for investigation
- WHEN a failure is reproduced
- THEN richer local evidence is collected without changing the functional behavior of the app

### Requirement: Troubleshooting UX and Documentation

The system MUST provide a documented local troubleshooting path for maintainers.

#### Scenario: Maintainer investigates a hang
- GIVEN a maintainer sees a stalled shutdown or non-exiting test run
- WHEN they follow the troubleshooting flow
- THEN they can capture a local diagnostics report and map the evidence to the affected subsystem
