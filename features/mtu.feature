Feature: Send simple files with limited network bandwidth

  Scenario: Send 100MB file with MTU 9000
    Given diode is started with max throughput of 100 Mb/s and MTU 9000
    When diode-file-send file A of size 100MB
    Then diode-file-receive file A in 5 seconds

  Scenario: Send multiple 100MB file with MTU 9000, 3 files received
    Given diode is started with max throughput of 100 Mb/s and MTU 9000
    When diode-file-send file A of size 100MB
    And diode-file-send file B of size 100MB
    And diode-file-send file C of size 100MB
    Then diode-file-receive file A in 5 seconds
    And diode-file-receive file B in 5 seconds
    And diode-file-receive file C in 5 seconds

