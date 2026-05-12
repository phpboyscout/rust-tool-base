Feature: rtb-vcs — Repo lifecycle (init / open / status)
  Foundation slice (v0.5 commit 1). Downstream tools need to
  initialise a fresh repository (the scaffolder's `rtb new` flow
  per project memory), open an existing one (every other op), and
  query its working-tree status (drift detection). These three ops
  are the constructors + readiness probe for the rest of the `Repo`
  surface.

  Scenario: R1 — initialise a fresh repository at an empty path
    Given an empty temporary directory
    When I init a repository at that directory
    Then a ".git" directory exists at that path
    And opening the same path again succeeds

  Scenario: R2 — open an existing repository
    Given an existing repository at a temporary directory
    When I open that directory
    Then the open call returns Ok

  Scenario: R3 — open returns OpenFailed for a non-repository path
    Given a temporary directory with no repository
    When I attempt to open that directory
    Then the call fails with RepoError::OpenFailed
    And the OpenFailed error names the offending path

  Scenario: R4 — status of a freshly-initialised repository is clean
    Given a freshly-initialised repository
    When I query its status
    Then staged, unstaged, and untracked are all empty

  Scenario: R5 — status reports an untracked file
    Given a freshly-initialised repository
    And I create an untracked file "hello.txt"
    When I query its status
    Then untracked contains "hello.txt"
    And staged and unstaged are empty
