Feature: Test ability of lidi to restart properly by itself

  Scenario: Send one file restart receiver then send another file, no file lost
    Given there is a limited network bandwidth of 100 Mb/s
    And lidi is started with max throughput of 90mbit
    When lidi-file-send file A of size 100KB
    And lidi-file-receive file A in 5 seconds
    And lidi-receive is restarted
    And lidi-file-send file B of size 100KB
    Then lidi-file-receive file B in 5 seconds

  Scenario: Send one file restart sender then send another file, no file lost
    Given there is a limited network bandwidth of 100 Mb/s
    And lidi is started with max throughput of 90mbit
    When lidi-file-send file A of size 100KB
    And lidi-file-receive file A in 5 seconds
    And lidi-send is restarted
    And lidi-file-send file B of size 100KB
    Then lidi-file-receive file B in 5 seconds
  
  Scenario: Send 3x100MB file with lidi-send restarts during transfer, first and last files transmitted
    Given there is a limited network bandwidth of 100 Mb/s
    And lidi is started with max throughput of 90mbit
    When lidi-file-send file A of size 100MB
    And lidi-send restarts while lidi-file-send file B of size 100MB
    And lidi-file-send file C of size 100MB
    Then lidi-file-receive file A in 15 seconds
    Then lidi-file-receive file C in 15 seconds

  Scenario: Send 3x100MB file with lidi-receive restarts during transfer, first and last files transmitted
    Given there is a limited network bandwidth of 100 Mb/s
    And lidi is started with max throughput of 90mbit
    When lidi-file-send file A of size 100MB
    And lidi-receive restarts while lidi-file-send file B of size 100MB
    And lidi-file-send file C of size 100MB
    Then lidi-file-receive file A in 15 seconds
    Then lidi-file-receive file C in 15 seconds

  Scenario: Send one file restart file_receiver then send another file, no file lost
    Given there is a limited network bandwidth of 100 Mb/s
    And lidi is started with max throughput of 90mbit
    When lidi-file-send file A of size 100KB
    And lidi-file-receive file A in 5 seconds
    And lidi-file-receive is restarted
    And lidi-file-send file B of size 100KB
    Then lidi-file-receive file B in 5 seconds
