Feature: Send simple files with network interrupts

  @fail
  Scenario: Send 3x100KB file with network interrupt, 2 first files lost, last one transmitted
    Given there is a network interrupt of 100KB after 50KB
    And there is a limited network bandwidth of 100 Mb/s
    And diode is started with max throughput of 90 Mb/s
    When diode-file-send file A of size 100KB
    And diode-file-send file B of size 100KB
    And diode-file-send file C of size 100KB
    Then diode-file-receive file C in 5 seconds

  @fail
  Scenario: Send 3x1MB file with network interrupt, 2 first files lost, last one transmitted
    Given there is a network interrupt of 1MB after 500KB
    And there is a limited network bandwidth of 100 Mb/s
    And diode is started with max throughput of 90 Mb/s
    When diode-file-send file A of size 1MB
    And diode-file-send file B of size 1MB
    And diode-file-send file C of size 1MB
    Then diode-file-receive file C in 5 seconds

  @fail
  Scenario: Send 3x100MB file with network interrupt, 2 first files lost, last one transmitted
    Given there is a network interrupt of 100MB after 50MB
    And there is a limited network bandwidth of 100 Mb/s
    And diode is started with max throughput of 90 Mb/s
    When diode-file-send file A of size 100MB
    And diode-file-send file B of size 100MB
    And diode-file-send file C of size 100MB
    Then diode-file-receive file C in 5 seconds

