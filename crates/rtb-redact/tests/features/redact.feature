Feature: rtb-redact — free-form secret redaction
  Callers write free-form log lines, telemetry attrs, and error
  messages. rtb-redact scrubs those strings of common secret shapes
  before they cross a boundary we don't control.

  Scenario: S1 — connection-string URL redacts only the userinfo
    Given the input is "postgres://app:hunter2@db.internal/mydb"
    When I redact the string
    Then the output is "postgres://[redacted]@db.internal/mydb"

  Scenario: S2 — mixed GitHub token and JWT in one line
    Given the input is "token=ghp_abc1234567890abcdef jwt=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
    When I redact the string
    Then the output does not contain "ghp_abc"
    And the output does not contain "eyJhbGciOiJIUzI1NiI"
    And the output contains "[redacted]"

  Scenario: S3 — error message carrying URL and Authorization header
    Given the input is "request failed: Authorization: Bearer sk-ant-api03-abcdef0123456789abcdef to https://user:pw@api.example.com/v1"
    When I redact the string
    Then the output contains "Bearer [redacted]"
    And the output contains "https://[redacted]@api.example.com"
    And the output contains "request failed:"
    And the output does not contain "sk-ant-api03-abcdef"
    And the output does not contain "user:pw"

  Scenario: S4 — PEM block embedded in a multi-line log
    Given the input is a multi-line PEM log
    When I redact the string
    Then the output contains "-----BEGIN PRIVATE KEY-----"
    And the output contains "[redacted]"
    And the output contains "-----END PRIVATE KEY-----"
    And the output contains "routine log entry"
    And the output does not contain "MIIEvQIBADAN"

  Scenario: S5 — Google Maps URL with a key parameter
    Given the input is "https://maps.googleapis.com/maps/api/js?key=AIzaSyABCDEFGHIJKLMNOPQR&callback=init"
    When I redact the string
    Then the output contains "key=[redacted]"
    And the output does not contain "AIzaSyABCDEFGHIJKLMNOPQR"

  Scenario: S6 — custom token prefix without a named rule and without
    40+ opaque chars passes through
    Given the input is "custom CUSTOMPREFIX-shortish done"
    When I redact the string
    Then the output is "custom CUSTOMPREFIX-shortish done"
