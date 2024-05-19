Feature: Send simple files with limited network bandwidth

  Scenario: Send 100MB file with max network of 100 Mb/s
    Given there is a limited network bandwidth of 100 Mb/s
    And diode is started with max throughput of 100 Mb/s
    When diode-file-send file A of size 100MB
    Then diode-file-receive file A in 5 seconds

  Scenario: Send multiple 100MB file with max network of 100 Mb/s, 3 files received
    Given there is a limited network bandwidth of 100 Mb/s
    And diode is started with max throughput of 90 Mb/s
    When diode-file-send file A of size 100MB
    And diode-file-send file B of size 100MB
    And diode-file-send file C of size 100MB
    Then diode-file-receive file A in 5 seconds
    And diode-file-receive file B in 5 seconds
    And diode-file-receive file C in 5 seconds

