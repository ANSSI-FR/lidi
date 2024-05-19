Feature: Send simple files with network drop

  Scenario: Send a 100K file with drop
    Given there is a network drop of 5 %
    And diode is started
    When diode-file-send file A of size 100KB
    Then diode-file-receive file A in 5 seconds

  Scenario: Send a 200M file with drop
    Given there is a network drop of 5 %
    And diode is started with max throughput of 100 Mb/s
    When diode-file-send file A of size 200MB 
    Then diode-file-receive file A in 5 seconds

