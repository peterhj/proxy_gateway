CARGO := cargo --offline
TARGETS := proxy_gateway
EXAMPLES := bench_thread,bench_timer

.PHONY: all debug rel release clean

all: debug

debug:
	$(CARGO) build --lib --bins --examples
	mkdir -p debug_dist
	cp -p target/debug/${TARGETS} target/debug/examples/{${EXAMPLES}} debug_dist/
	strip debug_dist/${TARGETS} debug_dist/{${EXAMPLES}}
	mkdir -p ../debug_dist
	cp -p debug_dist/${TARGETS} debug_dist/{${EXAMPLES}} ../debug_dist/

rel: release

release:
	$(CARGO) build --release --lib --bins --examples
	mkdir -p dist
	cp -p target/release/${TARGETS} target/release/examples/{${EXAMPLES}} dist/
	strip dist/${TARGETS} dist/{${EXAMPLES}}
	mkdir -p ../dist
	cp -p dist/${TARGETS} dist/{${EXAMPLES}} ../dist/

clean:
	rm -f debug_dist/*
	rm -f dist/*
	rm -rf target
