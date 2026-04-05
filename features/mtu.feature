Feature: Send simple files with limited network bandwidth

  Scenario: Send 100MB file with MTU 9000
    Given lidi is started with max throughput of 100mbit and MTU 9000
    When lidi-file-send file A of size 100MB
    Then lidi-file-receive file A in 5 seconds

  Scenario: Send multiple 100MB file with MTU 9000, 3 files received
    Given lidi is started with max throughput of 100mbit and MTU 9000
    When lidi-file-send file A of size 100MB
    And lidi-file-send file B of size 100MB
    And lidi-file-send file C of size 100MB
    Then lidi-file-receive file A in 5 seconds
    And lidi-file-receive file B in 5 seconds
    And lidi-file-receive file C in 5 seconds

