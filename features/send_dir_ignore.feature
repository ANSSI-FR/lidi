Feature: Check lidi-dir-send is not sending ignored files

  Scenario: Copy a dot file with lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with watch and ignore dot files
    When we copy a file .A of size 1KB
    Then lidi-file-receive no file .A in 5 seconds
    Then file .A is in source directory 

  Scenario: Move a dot file with lidi-dir-send
    Given lidi is started with max throughput of 100mbit
    And lidi-dir-send is started with watch and ignore dot files
    When we move a file .A of size 1KB
    Then lidi-file-receive no file .A in 5 seconds
    Then file .A is in source directory 

