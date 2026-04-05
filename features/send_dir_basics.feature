Feature: Check lidi-dir-send is sending one or multiple files with copy or move

  Scenario: Copy a 1K file with lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with watch
    When we copy a file A of size 1KB
    Then lidi-file-receive file A in 5 seconds

  Scenario: Copy multiple 1K files with lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with watch
    When we copy a file A of size 1KB
    When we copy a file B of size 1KB
    When we copy a file C of size 1KB
    Then lidi-file-receive file A in 5 seconds
    Then lidi-file-receive file B in 5 seconds
    Then lidi-file-receive file C in 5 seconds

  Scenario: Move a 1K file with lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with watch
    When we move a file A of size 1KB
    Then lidi-file-receive file A in 5 seconds

  Scenario: Move multiple 1K file with lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with watch
    When we move a file A of size 1KB
    When we move a file B of size 1KB
    When we move a file C of size 1KB
    Then lidi-file-receive file A in 5 seconds
    Then lidi-file-receive file B in 5 seconds
    Then lidi-file-receive file C in 5 seconds
