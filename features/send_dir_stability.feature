Feature: Check diode-send-dir is still working after a diode-send restart

  Scenario: send-dir is still working after diode-send restarts
    Given diode with send-dir is started
    When we copy a file A of size 10KB
    And diode-file-receive file A in 5 seconds
    And diode-send is restarted
    And we copy a file B of size 10KB
    Then diode-file-receive file B in 5 seconds

  Scenario: One file in dir before starting diode-send-dir
    Given diode is started
    When we copy a file A of size 10KB
    And diode-send-dir is started
    And we copy a file B of size 10KB
    Then diode-file-receive file A in 5 seconds
    And diode-file-receive file B in 5 seconds

  Scenario: Many files in dir before starting diode-send-dir
    Given diode is started with max throughput of 100 Mb/s
    When we copy 500 files of size 1KB
    And diode-send-dir is started
    And we copy 500 files of size 1KB
    Then diode-file-receive all files in 60 seconds
