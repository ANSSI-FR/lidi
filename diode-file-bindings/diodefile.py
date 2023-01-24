from cffi import FFI

ffi = FFI()
ffi.cdef("""
    typedef void* diode_config;
    diode_config diode_new_config(const char* addr, uint32_t buffer_size);
    void diode_free_config(diode_config config);
    uint32_t diode_send_file(diode_config config, const char* filepath);
    void diode_receive_files(diode_config config, const char* outputdir);
""")

C = ffi.dlopen("../target/release/libdiode_file_bindings.so")

class DiodeConfig(object):
    def __init__(self, addr, buffer_size):
        c_addr = ffi.new("char[]", addr.encode("utf-8"));
        self.__obj = C.diode_new_config(c_addr, buffer_size)

    def __del__(self):
        C.diode_free_config(self.__obj)
        self__obj = None

class DiodeSender(DiodeConfig):
    def __init__(self, to_tcp, buffer_size = 4194304):
        super().__init__(to_tcp, buffer_size)

    def send_file(self, filepath):
        c_filepath = ffi.new("char[]", filepath.encode("utf-8"))
        return C.diode_send_file(self._DiodeConfig__obj, c_filepath)
    
    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        del self

class DiodeReceiver(DiodeConfig):
    def __init__(self, from_tcp, buffer_size = 4194304):
        super().__init__(from_tcp, buffer_size)

    def receive_files(self, outputdir):
        c_outputdir = ffi.new("char[]", outputdir.encode("utf-8"))
        return C.diode_receive_files(self._DiodeConfig__obj, c_outputdir)
    
    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        del self

