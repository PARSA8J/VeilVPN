package main

import "syscall"

// NewLazy mirrors syscall.NewLazyDLL (kept tiny for clarity)
func NewLazy(name string) *syscall.LazyDLL { return syscall.NewLazyDLL(name) }

func syscallUTF16FromString(s string) ([]uint16, error) {
	return syscall.UTF16FromString(s)
}

func syscallUTF16ToString(u []uint16) string {
	return syscall.UTF16ToString(u)
}
