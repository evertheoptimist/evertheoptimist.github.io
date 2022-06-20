.PHONY: site
site:
	cd mksite && \
	rm -rf ../out.tmp && \
	mkdir ../out.tmp && \
	cargo run -- ../articles ../out.tmp && \
	ln -s ../articles/assets ../static ../out.tmp && \
	rm -rf ../out && \
	mv ../out.tmp ../out

.PHONY: clean
clean:
	rm -rf ./out && cd mksite && cargo clean
