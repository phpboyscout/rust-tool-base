Feature: rtb-telemetry — opt-in anonymous telemetry pipeline
  As a tool author
  I want a pluggable, two-level opt-in telemetry pipeline
  So that users control their data and sink choice is my call

  Scenario: S1 — Disabled context emits nothing
    Given a TelemetryContext with a MemorySink and Disabled policy
    When I record "should.not.emit"
    Then the sink recorded 0 events

  Scenario: S2 — Enabled context emits one event
    Given a TelemetryContext with a MemorySink and Enabled policy
    When I record "enabled.one"
    Then the sink recorded 1 events
    And the last event name is "enabled.one"

  Scenario: S3 — record_with_attrs attaches the attrs map
    Given a TelemetryContext with a MemorySink and Enabled policy
    When I record "with.attrs" with attrs "command=deploy;outcome=ok"
    Then the last event attribute "command" is "deploy"
    And the last event attribute "outcome" is "ok"

  Scenario: S4 — Two sequential records appear in order
    Given a TelemetryContext with a MemorySink and Enabled policy
    When I record "first"
    And I record "second"
    Then the sink recorded 2 events
    And the event at index 0 has name "first"
    And the event at index 1 has name "second"

  Scenario: S5 — FileSink writes JSONL
    Given a new FileSink with a temporary path
    When I emit an event named "disk.one"
    Then the file contains a JSON line with name "disk.one"

  Scenario: S6 — MachineId::derive with a fixed salt is stable
    Given I derive the machine id with salt "bdd-salt-1"
    When I derive the machine id with salt "bdd-salt-1" again
    Then the two ids are equal
    And each id is 64 hex characters

  Scenario: S7 — FileSink redacts args and err_msg before writing to disk
    Given a new FileSink with a temporary path
    When I emit an event named "cmd.run" with args "deploy --token ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" and err_msg "auth: Bearer ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    Then the file does not contain "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    And the file contains a JSON line with name "cmd.run"

  @remote-sinks
  Scenario: S8 — HttpSink posts redacted JSON to the configured endpoint
    Given a wiremock server that accepts telemetry at "/ingest"
    When I emit an HttpSink event named "cmd.http" with err_msg "auth: Bearer ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    Then the wiremock server received 1 request
    And the received body contains severity ERROR
    And the received body does not contain "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

  @remote-sinks
  Scenario: S9 — OtlpSink rejects an endpoint with no recognised scheme
    When I build an OtlpSink targeting "tcp://not-a-collector"
    Then OtlpSink construction fails with an Otlp error
