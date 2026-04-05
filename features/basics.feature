Feature: Basic lidi functionality

  Scenario: Send multiple 10K files without drop
    Given lidi is started with max throughput of 100mbit
    When lidi-file-send file A of size 10KB
    When lidi-file-send file B of size 10KB
    When lidi-file-send file C of size 10KB
    Then lidi-file-receive file A in 5 seconds
    Then lidi-file-receive file B in 5 seconds
    Then lidi-file-receive file C in 5 seconds

  Scenario: Send multiple 10M files without drop
    Given lidi is started with max throughput of 100mbit
    When lidi-file-send file A of size 10MB
    When lidi-file-send file B of size 10MB
    When lidi-file-send file C of size 10MB
    Then lidi-file-receive file A in 5 seconds
    Then lidi-file-receive file B in 5 seconds
    Then lidi-file-receive file C in 5 seconds

  Scenario: Send multiple 100M files without drop
    Given lidi is started with max throughput of 100mbit
    When lidi-file-send file A of size 100MB
    When lidi-file-send file B of size 100MB
    When lidi-file-send file C of size 100MB
    Then lidi-file-receive file A in 5 seconds
    Then lidi-file-receive file B in 5 seconds
    Then lidi-file-receive file C in 5 seconds
