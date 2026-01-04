Sending UDP with Lidi
=====================

.. code-block:: none

   Send UDP datagrams to diode-receive-udp.

   Usage: diode-send-udp [OPTIONS] --from <ip:port> <--to-tcp <ip:port>|--to-unix <path>>

   Options:
         --log-level <Off|Error|Warn|Info|Debug|Trace>  Log level [default: Info]
         --to-tcp <ip:port>                             TCP address and port to connect to diode-send
         --to-unix <path>                               Path to Unix socket to connect to diode-send
         --from <ip:port>                               IP address and port to receive UDP packets
     -h, --help                                         Print help

.. code-block:: none

   Receive UDP packets sent by diode-send-udp.

   Usage: diode-receive-udp [OPTIONS] --to-bind <ip:port> --to <ip:port> <--from-tcp <ip:port>|--from-unix <path>>

   Options:
         --log-level <Off|Error|Warn|Info|Debug|Trace>
             Log level [default: Info]
         --from-tcp <ip:port>
             IP address and port to accept TCP connections from diode-receive
         --from-unix <path>
             Path of Unix socket to accept Unix connections from diode-receive
         --to-bind <ip:port>
             IP address and port to send UDP packets from
         --to <ip:port>
             IP address and port to send UDP packets to
     -h, --help
             Print help   
