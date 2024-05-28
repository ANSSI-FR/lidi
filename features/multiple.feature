Feature: Send simple small files at the same time

  # rate limit is not necessary because when using send-file
  # with multiple files at once, it will send all packets
  # in a single connexion instead of a single file per connection
  Scenario: Send 10x1K file without drop
    Given diode is started
    When diode-file-send 10 files of size 1KB
    Then diode-file-receive 10 files in 5 seconds

  Scenario: Send 10x10K file without drop
    Given diode is started with max throughput of 100 Mb/s
    When diode-file-send 10 files of size 10KB
    Then diode-file-receive 10 files in 5 seconds

  Scenario: Send 10x100K file without drop
    Given diode is started with max throughput of 100 Mb/s
    When diode-file-send 10 files of size 100KB
    Then diode-file-receive 10 files in 5 seconds

