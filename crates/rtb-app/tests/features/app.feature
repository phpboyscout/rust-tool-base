Feature: rtb-app — application context, metadata, features, commands
  As a downstream rtb-* crate or a tool author
  I want a strongly-typed App context, tool metadata, feature gating, and a command registry
  So that my CLI tool is cheap to wire up and hard to misuse

  Scenario: S1 — minimal ToolMetadata is buildable and renders for debugging
    Given a ToolMetadata built with name "mytool" and summary "does things"
    Then its name is "mytool"
    And its summary is "does things"
    And its description is the empty string
    And it has no release source

  Scenario: S2 — ToolMetadata round-trips through YAML preserving GitHub defaults
    Given a ToolMetadata with a GitHub release source owner "me" repo "it"
    When I serialise it to YAML and deserialise it back
    Then the release source host is "github.com"
    And the name is "mytool"

  Scenario: S3 — runtime feature gating: disable Update and enable AI
    Given the default feature set
    When I disable "Update"
    And I enable "Ai"
    Then "Ai" is enabled
    And "Update" is not enabled
    And "Init" is still enabled

  Scenario: S4 — HelpChannel::Slack renders a natural support footer
    Given a Slack help channel with team "platform" and channel "cli-tools"
    When I format the footer
    Then the footer reads "support: slack #cli-tools (in platform)"

  Scenario: S5 — registering a command makes it observable in BUILTIN_COMMANDS
    Given the process has registered a command named "rtb-app-test-cmd"
    When I iterate BUILTIN_COMMANDS
    Then the list contains "rtb-app-test-cmd"

  Scenario Outline: S6 — is_development is true for pre-1.0 or pre-release builds
    Given a version "<version>"
    Then it <verdict> considered a development build

    Examples:
      | version        | verdict |
      | 0.1.0          | is      |
      | 0.0.0          | is      |
      | 1.0.0-alpha    | is      |
      | 1.2.3-dev.5    | is      |
      | 1.0.0          | is not  |
      | 2.3.4          | is not  |

  Scenario: S7 — Command::run's error propagates as a miette::Report
    Given a command whose run method returns an error "nope"
    When I invoke it via the Command trait
    Then the result is an Err
    And the rendered diagnostic contains "nope"

  Scenario: S8 — ReleaseSource::Github deserialises with and without a host
    Given the YAML "type: github\nowner: me\nrepo: it"
    When I deserialise it as a ReleaseSource
    Then it is a Github source with host "github.com"

    Given the YAML "type: github\nowner: me\nrepo: it\nhost: github.example.com"
    When I deserialise it as a ReleaseSource
    Then it is a Github source with host "github.example.com"
