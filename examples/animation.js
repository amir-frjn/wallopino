class BlackHole extends HTMLElement {
    connectedCallback() {
        this.canvas = this.querySelector(".js-canvas");
        this.ctx = this.canvas.getContext("2d");

        this.resize();

        window.addEventListener("resize", () => {
            this.resize();
        });

        requestAnimationFrame((time) => this.tick(time));
    }

    resize() {
        const rect = this.getBoundingClientRect();

        this.canvas.width = rect.width * devicePixelRatio;
        this.canvas.height = rect.height * devicePixelRatio;

        this.width = rect.width;
        this.height = rect.height;

        this.ctx.setTransform(
            devicePixelRatio,
            0,
            0,
            devicePixelRatio,
            0,
            0
        );
    }

    tick(time) {
        const ctx = this.ctx;

        ctx.clearRect(0, 0, this.width, this.height);

        const x = this.width / 2;
        const y = this.height / 2;

        const radius =
            100 + Math.sin(time * 0.002) * 20;

        ctx.beginPath();
        ctx.arc(x, y, radius, 0, Math.PI * 2);

        ctx.strokeStyle = "#00cccc";
        ctx.lineWidth = 2;
        ctx.stroke();

        requestAnimationFrame((t) => this.tick(t));
    }
}

customElements.define("black-hole", BlackHole);