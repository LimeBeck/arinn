# EWMH shell discovery for unmapped root children

Discover the shell window among direct root children when xfwm4 omits unmapped
windows from `_NET_CLIENT_LIST`, so global Home works while the Qt shell stays
`Window.Hidden`.

Fixes https://github.com/LimeBeck/arinn/issues/283
