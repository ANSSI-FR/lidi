# implementation of a virtual file system used to rate limit file read of diode-send-file

import os
import time
import multiprocessing
from fusepy import FUSE, Operations

class ThrottledFS(Operations):
    def __init__(self, root, read_rate_limit):
        self.root = root
        self.read_rate_limit = read_rate_limit  # bytes per second
        self.last_read_time = time.time()

    def _full_path(self, partial):
        if partial.startswith("/"):
            partial = partial[1:]
        path = os.path.join(self.root, partial)
        return path

    def open(self, path, flags):
        full_path = self._full_path(path)
        return os.open(full_path, flags)

    def read(self, path, length, offset, fh):
        current_time = time.time()
        elapsed = current_time - self.last_read_time

        sleep_time = (length / self.read_rate_limit) - elapsed

        if sleep_time <= 0:
            sleep_time = (length / self.read_rate_limit)

        time.sleep(sleep_time)

        os.lseek(fh, offset, os.SEEK_SET)
        buf = os.read(fh, length)
        self.last_read_time = time.time()

        return buf

    def getattr(self, path, fh=None):
        full_path = self._full_path(path)
        st = os.lstat(full_path)
        return dict((key, getattr(st, key)) for key in (
            'st_atime', 'st_ctime', 'st_gid', 'st_mode', 'st_mtime',
            'st_nlink', 'st_size', 'st_uid'))

    def readdir(self, path, fh):
        full_path = self._full_path(path)
        dirents = ['.', '..']
        if os.path.isdir(full_path):
            dirents.extend(os.listdir(full_path))
        for r in dirents:
            yield r

class ThrottledFSProcess():
    def __init__(self, mountpoint, root, read_rate_limit):
        self.__proc = multiprocessing.Process(target=throttled_fs_main, args=(mountpoint, root, read_rate_limit))

    def start(self):
        self.__proc.start()

    def kill(self):
        # Terminate the process
        self.__proc.terminate()  # sends a SIGTERM

def throttled_fs_main(mountpoint, root, read_rate_limit):
    FUSE(ThrottledFS(root, read_rate_limit), mountpoint, nothreads=True, foreground=True)

if __name__ == '__main__':
    import sys
    if len(sys.argv) != 4:
        print("Usage: {} <mountpoint> <root> <read_rate_limit>".format(sys.argv[0]))
        sys.exit(1)

    mountpoint = sys.argv[1]
    root = sys.argv[2]
    read_rate_limit = int(sys.argv[3])

    throttled_fs_main(mountpoint, root, read_rate_limit)
