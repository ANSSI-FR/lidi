# Hardening

This document describes the various steps that are taken by lidi in
order to protect itself.

## Prerequisites

Part of the hardening relies on the ability of unprivileged processes to create
user namespaces.

Therefore on distributions (i.e. Debian) disabling that ability, you need to
enable it.

```
sudo sysctl kernel.unprivileged_userns_clone=1
```

## Hardening the controller process with systemd

The main way we harden the controller process is through the provided systemd service.

The first thing we do is to rely on systemd's DynamicUser feature to run as an
unprivileged user. We then use the StateDirectory feature to store all of the
counter's state. With CapabilityBoundingSet, we ensure we do not have any
capability and we can't gain any.

The second thing we do is to setup a minimal seccomp profile using
SystemCallFilter. Here are all of the allowed syscalls with an explanation as to
why they are enabled:

  * We use extended attributes to store metadata:
    - fsetxattr
    - lgetxattr
  * The master process is "chrooted" inside a directory:
    - pivot_root
    - mount
    - chdir
    - umount2
    - rmdir
    - unshare
  * We use timerfd to update our leaky bucket:
    - timerfd_create
    - timerfd_settime
  * We use epoll to poll sockets:
    - epoll_wait
    - epoll_create
    - epoll_ctl
  * We get the number of CPUs on the system to decide how many workers we need:
    - sched_getaffinity
  * We create new processes and then apply seccomp profiles to them:
    - clone
    - prctl
    - seccomp
  * Rust memory management:
    - brk
    - mmap
    - mremap
    - munmap
    - mprotect
  * We manage signals:
    - sigaltstack
    - rt_sigaction
    - rt_sigprocmask
  * We use network sockets and socketpairs:
    - socketpair
    - socket
    - sendto
    - bind
    - sendmsg
    - recvmsg
    - sendto
    - recvfrom
  * We use inotify to watch for new files:
    - inotify_init1
    - inotify_add_watch
  * A bunch of file related syscalls:
    - access
    - openat
    - fstat
    - stat
    - read
    - write
    - ioctl
    - mkdir
    - getdents64
    - close
    - rename
    - lstat
    - chmod
    - fcntl
  * Some syscalls I haven't identified the use of yet:
    - arch_prctl
    - getrandom
    - getresuid
    - futex

## Further hardening of the controller process

The first hardening done through systemd is complemented by further hardening
at runtime. After parsing all of its arguments, the controller will "chroot" itself
using a mount namespace into the StateDirectory provided by systemd.

## Hardening of the worker processes

In addition to the hardening performed on the controller process which is inherited,
we further harden the worker processes. First, they are spawned in a new network
namespace.

Essentially we apply a more restrictive seccomp filter as soon as we are done
setting up the worker. Here are the syscalls we keep and the reason why:

  * To wait on our epoll:
    - epoll_wait
  * To move files between staging/complete/transfer/failed directories:
    - rename
  * To handle network and file I/O:
    - recvmsg
    - sendto
    - read
    - write
  * To close the file handle passed from master at the end:
    - close
  * To generate UUIDs:
    - getrandom
  * For rusts' memory management:
    - brk
    - mmap
    - mremap
    - munmap
    - sigaltstack
