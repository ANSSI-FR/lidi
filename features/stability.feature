Feature: Test ability of lidi to restart properly by itself

  Scenario: Send one file restart receiver then send another file, no file lost
    Given there is a limited network bandwidth of 100 Mb/s
    And diode is started with max throughput of 90 Mb/s
    When diode-file-send file A of size 100KB
    And diode-file-receive file A in 5 seconds
    And diode-receive is restarted
    And diode-file-send file B of size 100KB
    Then diode-file-receive file B in 5 seconds

  Scenario: Send one file restart sender then send another file, no file lost
    Given there is a limited network bandwidth of 100 Mb/s
    And diode is started with max throughput of 90 Mb/s
    When diode-file-send file A of size 100KB
    And diode-file-receive file A in 5 seconds
    And diode-send is restarted
    And diode-file-send file B of size 100KB
    Then diode-file-receive file B in 5 seconds
  
  Scenario: Send 3x100MB file with diode-send restarts during transfer, first and last files transmitted
    Given there is a limited network bandwidth of 100 Mb/s
    And diode is started with max throughput of 90 Mb/s
    When diode-file-send file A of size 100MB
    And diode-send restarts while diode-file-send file B of size 100MB
    And diode-file-send file C of size 100MB
    Then diode-file-receive file A in 15 seconds
    Then diode-file-receive file C in 15 seconds

  Scenario: Send 3x100MB file with diode-receive restarts during transfer, first and last files transmitted
    Given there is a limited network bandwidth of 100 Mb/s
    And diode is started with max throughput of 90 Mb/s
    When diode-file-send file A of size 100MB
    And diode-receive restarts while diode-file-send file B of size 100MB
    And diode-file-send file C of size 100MB
    Then diode-file-receive file A in 15 seconds
    Then diode-file-receive file C in 15 seconds

