Feature: rtb-vcs — Repo write paths (clone / commit)
  v0.5 commit 4. Downstream tools need to clone a repository
  (scaffolder cloning a template, fetch-and-inspect tools) and to
  stage + commit changes (scaffolder initial commit, regenerator
  re-applying templates). Auth on clone is deferred to commit 5
  alongside fetch — the API surface is forward-compatible via
  CloneOptions.

  Scenario: C1 — anonymous clone from a local file:// URL
    Given a 3-commit upstream repository
    When I clone it via file:// into an empty destination
    Then the destination has a ".git" directory
    And the cloned HEAD message is "third"

  Scenario: M1 — commit creates an initial commit
    Given a freshly-initialised repository with local identity
    And I write "README.md" containing "hello"
    When I commit ["README.md"] with message "initial"
    Then the new commit is the repository's HEAD
    And the HEAD commit message is "initial"

  Scenario: M2 — commit fails when given no paths
    Given a freshly-initialised repository with local identity
    When I attempt to commit [] with message "nothing"
    Then the call fails with RepoError::CommitFailed
