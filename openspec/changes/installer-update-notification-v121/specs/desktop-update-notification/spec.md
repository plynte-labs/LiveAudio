# Desktop Update Notification Specification

## Purpose

Define how the desktop app discovers, preserves, presents, and hands off available app updates to the launcher.

## Requirements

### Requirement: Reliable Update Availability State

The system MUST preserve a confirmed newer release tag independently from transient UI widgets.

#### Scenario: Update found before main screen exists

- GIVEN the app is showing the welcome screen
- WHEN the update check confirms a newer release tag
- THEN the app MUST retain the pending update tag
- AND the user MUST be notified after the main screen is available

#### Scenario: Main screen rebuild after update detection

- GIVEN a pending update tag exists
- WHEN the main screen is destroyed and rebuilt
- THEN the update notice MUST be rendered again for that pending tag

### Requirement: User-Controlled Update Prompt

The system MUST ask the user before handing off to the launcher update flow.

#### Scenario: User accepts update

- GIVEN a newer release is available
- WHEN the user chooses to update now
- THEN the app MUST reuse the existing close confirmation before launcher hand-off
- AND the launcher MUST receive the selected release tag

#### Scenario: User postpones update

- GIVEN a newer release is available
- WHEN the user chooses to postpone
- THEN the app MUST NOT start the launcher update flow
- AND the app SHOULD avoid repeatedly prompting for the same tag in the same session

### Requirement: Non-Blocking Update Checks

The system MUST keep update checks non-blocking and safe for offline users.

#### Scenario: Offline startup

- GIVEN internet connectivity is unavailable
- WHEN the app starts
- THEN the app MUST NOT interrupt the user with update UI
- AND the app MUST continue normal startup and transcription behavior

#### Scenario: Network failure

- GIVEN GitHub release lookup fails
- WHEN the update check runs
- THEN the app MUST continue without blocking startup or transcription
- AND no update prompt MUST be shown

#### Scenario: No newer release

- GIVEN the latest GitHub release is not newer than the local version
- WHEN the update check completes
- THEN no update prompt MUST be shown

### Requirement: Daily and Six-Hour Update Freshness

The system SHOULD check for updates on the first app open of each day and MUST NOT perform successful-check attempts more often than every 6 hours afterward.

#### Scenario: First open of the day

- GIVEN the app has not checked for updates today
- WHEN the app starts with internet connectivity available
- THEN the app SHOULD run a non-blocking update check

#### Scenario: Check interval elapses while app remains open

- GIVEN the app remains open beyond the configured update check interval
- WHEN the interval elapses
- THEN the app SHOULD run another non-blocking update check

#### Scenario: Six-hour interval has not elapsed

- GIVEN a successful update check happened less than 6 hours ago
- WHEN the app starts or remains open
- THEN the app MUST NOT perform another release lookup
