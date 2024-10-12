# gc_fst
This library/binary extracts and rebuilds Gamecube ISO filesystems.

This is almost equivalent to the functionality given by [GCRebuilder](https://github.com/lunarsoap5/gcrebuilder)
and [gcmod](https://github.com/Addisonbean/gcmod).
It has much better error messages than either program, fewer bugs, and will work on linux, Windows, and probably OSX.

The `fs` command will attempt to modify the iso with as little io work as possible.
You can pass as many subcommands as you want at a time.
If the inserted file does not exist, then it will be created, along with any needed subdirectories.
The special files "ISO.hdr", "AppLoader.ldr", and "Start.dol" can be inserted and will replace the existing special file in the ISO,
and will not be inserted into the iso filesystem.

```
Usage: gc_fst extract <iso path>
       gc_fst rebuild <root path> [iso path]
       gc_fst set-header <ISO.hdr path | iso path> <game ID> [game title]

       gc_fst read <iso path> [

       gc_fst fs <iso path> [
           insert <path in iso> <path to file>
           delete <path in iso>
       ]*n
```

## Limitations

The `gc_fst` binary does not support editing metadata (banner image, game name, description, etc.) contained in [opening.bnr](https://hitmen.c02.at/files/yagcd/yagcd/chap14.html#sec14.1).
You can, however, use the library to create a new `opening.bnr` file.
See how [in this example](examples/create_opening_bnr.rs).

Note that `gc_fst` will always reconstruct the table of contents when rebuilding the iso.
Likewise, it will not emit a `Game.toc` file when extracting.
