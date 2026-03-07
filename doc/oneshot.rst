Oneshot Lidi
============

.. code-block:: none

   Read stdin and send it to diode-oneshot-receive (no need for diode-send nor diode-receive).
   
   Usage: diode-oneshot-send [OPTIONS] --to <ip>
   
   Options:
         --log-level <Off|Error|Warn|Info|Debug|Trace>
             Log level [default: Info]
         --flush
             Flush client data immediately
         --to <ip>
             IP address where to send UDP packets to diode-receive
         --to-ports <port[,port]*>...
             Ports on which to send UDP packets to diode-receive
         --to-bind <ip:port>
             Binding IP for UDP traffic [default: 0.0.0.0:0]
         --to-mtu <nb_bytes>
             MTU of the output UDP link [default: 1500]
         --mode <MODE>
             Send mode [default: native] [possible values: native, sendmsg, sendmmsg]
         --block <nb_bytes>
             Size of RaptorQ block in bytes [default: 734928]
         --repair <percentage>
             Percentage of RaptorQ repair data [default: 2]
         --hash
             Hash each client transfered data
     -h, --help
             Print help

.. code-block:: none

   Receive data from diode-oneshot-send and write them to stdout (no need for diode-send nor diode-receive).
   
   Usage: diode-oneshot-receive [OPTIONS] --from <ip:port> --from-ports <port[,port]*>...
   
   Options:
         --log-level <Off|Error|Warn|Info|Debug|Trace>
             Log level [default: Info]
         --from <ip>
             IP address where to receive UDP packets from diode-send
         --from-ports <port[,port]*>...
             Ports on which to receive UDP packets from diode-send
         --from-mtu <nb_bytes>
             MTU of the input UDP link [default: 1500]
         --mode <MODE>
             Receive mode [default: native] [possible values: native, recvmsg, recvmmsg]
         --reset-timeout <seconds>
             Reset diode if no data are received after duration [default: 2]
         --flush
             Flush immediately data to clients
         --abort-timeout <seconds>
             Abort connections if no data received after duration (0 = no abort)
         --block <nb_bytes>
             Size of RaptorQ block in bytes [default: 734928]
         --repair <percentage>
             Percentage of RaptorQ repair data [default: 2]
         --min-repair <percentage>
             Minimal percentage of RaptorQ repair data required before decoding [default: 1]
     -h, --help
             Print help
