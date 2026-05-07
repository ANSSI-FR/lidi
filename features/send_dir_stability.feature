Feature: Check lidi-dir-send is still working after a lidi-send restart

  Scenario: send-dir is still working after lidi-send restarts
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with watch
    When we copy a file A of size 10KB
    And lidi-file-receive file A in 5 seconds
    And lidi-send is restarted
    And we copy a file B of size 10KB
    Then lidi-file-receive file B in 5 seconds

  Scenario: One file in dir before starting lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    When we copy a file A of size 10KB
    Given lidi-dir-send is started with watch
    When we copy a file B of size 10KB
    Then lidi-file-receive file A in 5 seconds
    And lidi-file-receive file B in 5 seconds

  Scenario: Many files in dir before starting lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    When we copy 500 files of size 1KB
    Given lidi-dir-send is started with watch
    When we copy 500 files of size 1KB
    Then lidi-file-receive all files in 60 seconds
