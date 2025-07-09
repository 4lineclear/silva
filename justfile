rl := "\"-Zsanitizer=address\
 -Zsanitizer=cfi\
 -Zsanitizer=dataflow\
 -Zsanitizer=hwaddress\
 -Zsanitizer=leak\
 -Zsanitizer=memory\
 -Zsanitizer=memtag\
 -Zsanitizer=shadow-call-stack\
 -Zsanitizer=thread\""

test *ARGS:
    RUST_FLAGS={{rl}} \
        cargo test {{ARGS}}

miri *ARGS:
    MIRIFLAGS="-Zmiri-disable-stacked-borrows" \
        cargo +nightly miri test {{ARGS}}

bench *ARGS:
    cargo bench {{ARGS}}

clean:
    cargo clean

perf-core:
    cargo build -r && \
    perf record --call-graph dwarf \
        ./target/release/silva

time-core:
    cargo build -r && \
    /usr/bin/time -v \
        ./target/release/silva

