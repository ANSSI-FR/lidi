Feature: Send several files at the same time

  Scenario: Send 10x1K file without drop
    Given lidi is started with max throughput of 100mbit
    When lidi-file-send 10 files of size 1KB
    Then lidi-file-receive all files in 5 seconds

  Scenario: Send 10x10K file without drop
    Given lidi is started with max throughput of 100mbit
    When lidi-file-send 10 files of size 10KB
    Then lidi-file-receive all files in 5 seconds

  Scenario: Send 10x100K file without drop
    Given lidi is started with max throughput of 100mbit
    When lidi-file-send 10 files of size 100KB
    Then lidi-file-receive all files in 5 seconds

  Scenario: Send 10x100M file without drop
    Given lidi is started with max throughput of 100mbit
    When lidi-file-send 10 files of size 100MB
    Then lidi-file-receive all files in 5 seconds
