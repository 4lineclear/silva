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

