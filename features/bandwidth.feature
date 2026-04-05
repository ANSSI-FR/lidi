Feature: Send simple files with limited network bandwidth

  Scenario: Send 100MB file with max network of 100 Mb/s
    Given there is a limited network bandwidth of 100 Mb/s
    And lidi is started with max throughput of 95mbit
    When lidi-file-send file A of size 100MB
    Then lidi-file-receive file A in 5 seconds

  Scenario: Send multiple 100MB file with max network of 100 Mb/s, 3 files received
    Given there is a limited network bandwidth of 100 Mb/s
    And lidi is started with max throughput of 95mbit
    When lidi-file-send file A of size 100MB
    And lidi-file-send file B of size 100MB
    And lidi-file-send file C of size 100MB
    Then lidi-file-receive file A in 5 seconds
    And lidi-file-receive file B in 5 seconds
    And lidi-file-receive file C in 5 seconds

  Scenario: Ensure bandwidth is never exceeded
    Given network bandwidth must not exceed 1 Mb/s 
    And lidi is started with max throughput of 990kbit
    When lidi-file-send file A of size 3MB
    Then lidi-file-receive file A in 30 seconds
