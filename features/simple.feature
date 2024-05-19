Feature: Send simple (without ratelimit), will probably fail with high volume

  # should not fail because very small
  Scenario: Send a 1K file without drop
    Given diode is started
    When diode-file-send file A of size 1KB
    Then diode-file-receive file A in 5 seconds

  # should not fail because very small
  Scenario: Send multiple 10K files without drop
    Given diode is started
    When diode-file-send file A of size 10KB
    When diode-file-send file B of size 10KB
    When diode-file-send file C of size 10KB
    Then diode-file-receive file A in 5 seconds
    Then diode-file-receive file B in 5 seconds
    Then diode-file-receive file C in 5 seconds

  # should not fail because small
  Scenario: Send multiple 100K files without drop
    Given diode is started
    When diode-file-send file A of size 100KB
    When diode-file-send file B of size 100KB
    When diode-file-send file C of size 100KB
    Then diode-file-receive file A in 5 seconds
    Then diode-file-receive file B in 5 seconds
    Then diode-file-receive file C in 5 seconds

  @fail
  ### May fail because sending fast
  Scenario: Send a 1M file without drop
    Given diode is started
    When diode-file-send file A of size 1MB
    Then diode-file-receive file A in 5 seconds

  @fail
  ### May fail because sending fast
  Scenario: Send multiple 1M files without drop
    Given diode is started
    When diode-file-send file A of size 1MB
    When diode-file-send file B of size 1MB
    When diode-file-send file C of size 1MB
    Then diode-file-receive file A in 5 seconds
    Then diode-file-receive file B in 5 seconds
    Then diode-file-receive file C in 5 seconds

  @fail
  ### Should fail, because sending too fast, many packets are lost
  Scenario: Send a 1G file without drop
    Given diode is started
    When diode-file-send file A of size 1GB
    Then diode-file-receive file A in 5 seconds

