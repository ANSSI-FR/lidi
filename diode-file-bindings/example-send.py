from diodefile import DiodeSender

with DiodeSender("127.0.0.1:5000") as diode:
    nb_bytes = diode.send_file("/tmp/file.txt")
    print(f"{nb_bytes} bytes sent (/tmp/file.txt)");
