Feature: Check diode-send-dir is not sending ignored files

  Scenario: Copy a dot file with diode-send-dir
    Given diode with send-dir is started
    When We copy a file .A of size 1KB
    Then diode-file-receive no file .A in 5 seconds
    Then file .A is in source directory 

  Scenario: Move a dot file with diode-send-dir
    Given diode with send-dir is started
    When We move a file .A of size 1KB
    Then diode-file-receive no file .A in 5 seconds
    Then file .A is in source directory 

