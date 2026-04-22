Feature: rtb-config — typed, layered configuration
  As a tool author
  I want compile-time-typed configuration with source layering
  So that my tool fails fast on bad input and never goes through untyped accessors

  Scenario: S1 — minimal config from an embedded default
    Given an embedded default YAML "port: 8080"
    When I build a Config typed as PortOnly
    Then the current snapshot's port is 8080

  Scenario: S2 — env overrides file overrides embedded
    Given an embedded default YAML "port: 8080"
    And a user file with content "port: 9090"
    And an environment variable "RTBCFG_BDD_PORT" set to "9999"
    When I build a Config with prefix "RTBCFG_BDD_" typed as PortOnly
    Then the current snapshot's port is 9999

  Scenario: S3 — missing required field surfaces a helpful diagnostic
    Given an embedded default YAML "other: x"
    When I build a Config typed as RequiresName and capture the error
    Then the error is a Parse variant
    And the error message mentions "name"

  Scenario: S4 — reload picks up updated file contents
    Given a user file with content "port: 1111"
    When I build a Config typed as PortOnly
    And I rewrite the user file to "port: 2222"
    And I reload the config
    Then the current snapshot's port is 2222

  Scenario: S5 — Config without a type parameter resolves to Config<()>
    Given a default Config with no type parameter
    Then the snapshot is the unit value

  Scenario: S6 — env prefix handles nested keys
    Given an embedded default YAML "http:\n  port: 1"
    And an environment variable "RTBCFG_BDD_NESTED_HTTP_PORT" set to "4242"
    When I build a Config with prefix "RTBCFG_BDD_NESTED_" typed as HttpOnly
    Then the nested http port is 4242
