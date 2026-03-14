Feature: Check diode-send-dir is sending one or multiple files with copy or move

  Scenario: Copy a 1K file with diode-send-dir
    Given diode with send-dir is started
    When we copy a file A of size 1KB
    Then diode-file-receive file A in 5 seconds

  Scenario: Copy multiple 1K files with diode-send-dir
    Given diode with send-dir is started
    When we copy a file A of size 1KB
    When we copy a file B of size 1KB
    When we copy a file C of size 1KB
    Then diode-file-receive file A in 5 seconds
    Then diode-file-receive file B in 5 seconds
    Then diode-file-receive file C in 5 seconds

  Scenario: Move a 1K file with diode-send-dir
    Given diode with send-dir is started
    When We move a file A of size 1KB
    Then diode-file-receive file A in 5 seconds

  Scenario: Move multiple 1K file with diode-send-dir
    Given diode with send-dir is started
    When we move a file A of size 1KB
    When we move a file B of size 1KB
    When we move a file C of size 1KB
    Then diode-file-receive file A in 5 seconds
    Then diode-file-receive file B in 5 seconds
    Then diode-file-receive file C in 5 seconds
