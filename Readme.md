# gc_fst
This library/binary extracts and rebuilds Gamecube ISO filesystems.

This is almost equivalent to the functionality given by [GCRebuilder](https://github.com/lunarsoap5/gcrebuilder)
and [gcmod](https://github.com/Addisonbean/gcmod).
It has much better error messages than either program, fewer bugs, and will work on linux.

gc_fst currently does not support editing metadata such as banners, game code, etc. but it is planned.

```
Usage: gc_fst extract <iso path>
       gc_fst rebuild <root path> [iso path]
```

Note that gc_fst always rebuilds the table of contents when rebuilding, and will not emit a Game.toc file when extracting.
