Feature: rtb-assets — overlay asset filesystem
  As a framework consumer
  I want to read from multiple asset layers with predictable precedence
  So that embedded defaults, user overrides, and in-memory fixtures can coexist

  Scenario: S1 — single in-memory layer round-trips a file
    Given a fresh Assets with a memory layer "defaults" containing "greeting.txt"="hello"
    When I open "greeting.txt" as text
    Then the text is "hello"

  Scenario: S2 — higher layer shadows lower for binary reads
    Given a fresh Assets with a memory layer "low" containing "x"="low"
    And an additional memory layer "high" containing "x"="high"
    When I open "x" as text
    Then the text is "high"

  Scenario: S3 — YAML merge across two layers combines nested maps
    Given a fresh Assets with a memory layer "low" containing "cfg.yaml"="name: lower\nnested:\n  host: localhost\n  port: 8080\n"
    And an additional memory layer "high" containing "cfg.yaml"="only_upper: yes\nnested:\n  port: 9090\n"
    When I merge-load "cfg.yaml" as YAML
    Then the merged host is "localhost"
    And the merged port is 9090
    And the merged name is "lower"
    And only_upper is "yes"

  Scenario: S4 — list_dir unions entries across layers
    Given a fresh Assets with a memory layer "low" containing "d/a.txt"="1" and "d/b.txt"="2"
    And an additional memory layer "high" containing "d/b.txt"="x" and "d/c.txt"="3"
    When I list the directory "d"
    Then the listing is "a.txt,b.txt,c.txt"

  Scenario: S5 — missing file surfaces NotFound
    Given a fresh Assets with a memory layer "m" containing "other.yaml"="x: 1"
    When I merge-load "missing.yaml" as YAML and capture the error
    Then the error is a NotFound variant for "missing.yaml"

  Scenario: S6 — malformed YAML surfaces Parse with the offending layer
    Given a fresh Assets with a memory layer "good" containing "c.yaml"="x: 1"
    And an additional memory layer "bad" containing "c.yaml"="::not yaml::\n\t::"
    When I merge-load "c.yaml" as YAML and capture the error
    Then the error is a Parse variant mentioning "bad"
