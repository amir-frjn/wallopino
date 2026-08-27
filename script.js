const easingUtils = {
    easeOutCubic(t) {
        return 1 - Math.pow(1 - t, 3);
    },

    easeOutExpo(t) {
        return t === 1
            ? 1
            : 1 - Math.pow(2, -10 * t);
    },

    easeInExpo(t) {
        return t === 0
            ? 0
            : Math.pow(2, 10 * (t - 1));
    },

    linear(t) {
        return t;
    }
};


/*
 * Elastic breathing.
 */
function elasticBreath(time, speed = 0.0012) {

    const t =
        (time * speed) %
        (Math.PI * 2);

    const breath =
        Math.sin(t);

    const elastic =
        Math.sin(t * 2.0) * 0.18 +
        Math.sin(t * 3.0) * 0.06;

    return (
        1 +
        breath * 0.018 +
        elastic *
        Math.max(breath, 0) *
        0.02
    );
}


class BlackHole extends HTMLElement {

    connectedCallback() {

        this.canvas =
            this.querySelector(".js-canvas");

        this.ctx =
            this.canvas.getContext("2d");

        this.startTime =
            performance.now();

        this.setSizes();

        this.bindEvents();

        requestAnimationFrame(
            this.tick.bind(this)
        );
    }


    bindEvents() {

        window.addEventListener(
            "resize",
            this.onResize.bind(this)
        );
    }


    onResize() {

        this.setSizes();
    }


    setSizes() {

        this.setCanvasSize();

        this.setGraphics();
    }


    setCanvasSize() {

        const rect =
            this.getBoundingClientRect();

        this.render = {

            width:
                rect.width,

            hWidth:
                rect.width * 0.5,

            height:
                rect.height,

            hHeight:
                rect.height * 0.5,

            dpi:
                window.devicePixelRatio || 1
        };


        this.canvas.width =
            this.render.width *
            this.render.dpi;


        this.canvas.height =
            this.render.height *
            this.render.dpi;
    }


    setGraphics() {

        this.setDiscs();

        this.setDots();
    }


    setDiscs() {

        this.discs = [];


        this.startDisc = {

            x:
                this.render.width * 0.5,

            y:
                0,

            w:
                this.render.width,

            h:
                this.render.height
        };


        const totalDiscs =
            150;


        for (
            let i = 0;
            i < totalDiscs;
            i++
        ) {

            const p =
                i / totalDiscs;


            const disc =
                this.tweenDisc({

                    p,

                    phase:
                        Math.random() *
                        Math.PI *
                        2,

                    wobble:
                        0.002 +
                        Math.random() *
                        0.004
                });


            this.discs.push(
                disc
            );
        }
    }


    setDots() {

        this.dots = [];


        const totalDots =
            20000;


        for (
            let i = 0;
            i < totalDots;
            i++
        ) {

            const disc =
                this.discs[
                Math.floor(
                    this.discs.length *
                    Math.random()
                )
                ];


            /*
             * Color distribution:
             *
             * ~82% cyan / teal
             * ~9% purple
             * ~9% neon green
             */
            const colorRoll =
                Math.random();


            let color;


            if (colorRoll < 0.09) {

                color =
                    "#a50ec5";

            } else if (colorRoll < 0.18) {

                color =
                    "#00ff9f";

            } else {

                color =
                    `rgb(
                        0,
                        ${150 + Math.random() * 50},
                        ${150 + Math.random() * 105}
                    )`;
            }


            const dot = {

                d:
                    disc,

                a:
                    0,

                p:
                    Math.random(),

                o:
                    0.35 +
                    Math.random() * 0.65,

                phase:
                    Math.random() *
                    Math.PI *
                    2,

                pulse:
                    0.5 +
                    Math.random() *
                    2.0,


                /*
                 * Faster and stronger twinkling.
                 */
                twinkleSpeed:
                    0.8 +
                    Math.random() *
                    2.8,

                twinklePhase:
                    Math.random() *
                    Math.PI *
                    2,


                /*
                 * Some particles twinkle more strongly.
                 */
                twinkleStrength:
                    0.35 +
                    Math.random() *
                    0.65,


                c:
                    color
            };


            this.dots.push(
                dot
            );
        }
    }


    tweenDisc(disc) {

        const {
            startDisc
        } = this;


        const scaleX =
            this.tweenValue(
                1,
                0,
                disc.p,
                "outCubic"
            );


        const scaleY =
            this.tweenValue(
                1,
                0,
                disc.p,
                "outExpo"
            );


        disc.sx =
            scaleX;

        disc.sy =
            scaleY;


        disc.w =
            startDisc.w *
            scaleX;

        disc.h =
            startDisc.h *
            scaleY;


        disc.x =
            startDisc.x;


        disc.y =
            startDisc.y +
            disc.p *
            startDisc.h;


        return disc;
    }


    tweenValue(
        start,
        end,
        p,
        ease = false
    ) {

        const delta =
            end - start;


        const easeFn =
            easingUtils[
            ease

                ? "ease" +
                ease
                    .charAt(0)
                    .toUpperCase() +
                ease.slice(1)

                : "linear"
            ];


        return (
            start +
            delta *
            easeFn(p)
        );
    }


    /*
     * Draw the very soft atmospheric
     * light surrounding the black hole.
     */
    drawGlow(time) {

        const {
            ctx
        } = this;

        const cx =
            this.render.hWidth;

        const cy =
            this.render.height * 0.52;


        /*
         * Slow breathing.
         */
        const breathe =
            elasticBreath(
                time,
                0.0012
            );


        /*
         * Outer halo radius.
         */
        const radius =

            Math.min(
                this.render.width,
                this.render.height
            ) *

            (
                0.38 +
                (breathe - 1) * 1.5
            );


        /*
         * Inner radius.
         *
         * Everything inside this region stays dark.
         */
        const innerRadius =
            radius * 0.30;


        const gradient =
            ctx.createRadialGradient(

                cx,
                cy,
                innerRadius,

                cx,
                cy,
                radius
            );


        /*
         * Completely transparent around
         * the event horizon.
         */
        gradient.addColorStop(
            0,
            "rgba(0, 0, 0, 0)"
        );


        /*
         * Start the very weak halo.
         */
        gradient.addColorStop(
            0.35,
            "rgba(0, 180, 190, 0.015)"
        );


        /*
         * Slightly stronger outer illumination.
         */
        gradient.addColorStop(
            0.60,
            "rgba(0, 140, 180, 0.025)"
        );


        gradient.addColorStop(
            0.80,
            "rgba(40, 80, 160, 0.018)"
        );


        /*
         * Fade completely away at the edge.
         */
        gradient.addColorStop(
            1,
            "rgba(0, 0, 0, 0)"
        );


        ctx.globalAlpha =
            1;

        ctx.fillStyle =
            gradient;


        ctx.beginPath();

        ctx.arc(
            cx,
            cy,
            radius,
            0,
            Math.PI * 2
        );

        ctx.fill();
    }


    /*
     * Draw orbital rings.
     *
     * They remain #0329.
     */
    drawDiscs(time) {

        const {
            ctx
        } = this;


        const breathe =
            elasticBreath(
                time,
                0.0012
            );


        ctx.strokeStyle =
            "#0329";


        ctx.lineWidth =
            1;


        this.discs.forEach(
            (disc) => {

                ctx.globalAlpha =
                    disc.a;


                ctx.beginPath();


                ctx.ellipse(

                    disc.x,

                    disc.y +
                    disc.h,

                    disc.w *
                    breathe,

                    disc.h *
                    breathe,

                    0,

                    0,

                    Math.PI * 2
                );


                ctx.stroke();


                ctx.closePath();
            }
        );
    }


    /*
     * Draw particles.
     */
    drawDots(time) {

        const {
            ctx
        } = this;


        this.dots.forEach(
            (dot) => {

                const {
                    d,
                    p,
                    c,
                    o,
                    phase,
                    pulse,
                    twinkleSpeed,
                    twinklePhase,
                    twinkleStrength
                } = dot;


                const _p =
                    d.sx *
                    d.sy;


                /*
                 * Existing particle pulse.
                 */
                const particlePulse =

                    0.70 +

                    Math.sin(

                        time *
                        0.001 *
                        pulse +

                        phase

                    ) *

                    0.30;


                /*
                 * Stronger visible twinkle.
                 *
                 * 0..1 range.
                 */
                const twinkleWave =

                    0.5 +

                    0.5 *

                    Math.sin(

                        time *
                        0.001 *
                        twinkleSpeed +

                        twinklePhase

                    );


                /*
                 * Smoothly vary opacity.
                 *
                 * Unlike before, some dots can now
                 * become noticeably brighter.
                 */
                const twinkle =

                    0.45 +

                    twinkleWave *
                    0.55 *
                    twinkleStrength;


                /*
                 * Occasional bright peak.
                 */
                const sparkle =

                    Math.pow(
                        twinkleWave,
                        3
                    ) *
                    0.35;


                const newA =

                    phase +

                    Math.PI *
                    2 *
                    p;


                /*
                 * Tiny orbital wobble.
                 */
                const wobble =

                    Math.sin(

                        time *
                        0.0007 +
                        phase

                    ) *

                    0.012;


                const x =

                    d.x +

                    Math.cos(newA) *

                    d.w *

                    (1 + wobble);


                const y =

                    d.y +

                    Math.sin(newA) *

                    d.h *

                    (1 + wobble);


                ctx.fillStyle =
                    c;


                /*
                 * Combine opacity layers.
                 */
                ctx.globalAlpha =

                    d.a *
                    o *
                    particlePulse *
                    (
                        twinkle +
                        sparkle
                    );


                /*
                 * Twinkling particles become
                 * slightly larger at their peak.
                 */
                const particleSize =

                    1 +
                    _p * 0.5 +

                    sparkle *
                    (
                        1.8 +
                        _p * 2.0
                    );


                ctx.beginPath();


                ctx.arc(

                    x,

                    y +
                    d.h,

                    particleSize,

                    0,

                    Math.PI * 2
                );


                ctx.fill();


                ctx.closePath();
            }
        );
    }


    /*
     * Move the orbital discs.
     */
    moveDiscs(time) {

        const breathing =
            elasticBreath(
                time,
                0.0012
            );


        this.discs.forEach(
            (disc) => {

                /*
                 * Move forward.
                 */
                disc.p =

                    (
                        disc.p +
                        0.0003
                    ) % 1;


                /*
                 * Recalculate geometry.
                 */
                this.tweenDisc(
                    disc
                );


                /*
                 * Gentle vertical motion.
                 */
                const verticalMotion =

                    Math.sin(

                        time *
                        0.001 +
                        disc.phase

                    ) *

                    disc.h *
                    0.008;


                disc.y +=
                    verticalMotion;


                /*
                 * Additional breathing.
                 */
                const scaleFactor =

                    0.985 +
                    breathing *
                    0.015;


                disc.w *=
                    scaleFactor;


                disc.h *=
                    scaleFactor;


                const p =
                    disc.sx *
                    disc.sy;


                let a =
                    1;


                /*
                 * Center fade.
                 */
                if (p < 0.01) {

                    a =
                        Math.pow(

                            Math.min(
                                p / 0.01,
                                1
                            ),

                            3
                        );


                    /*
                     * Outer fade.
                     */
                } else if (p > 0.2) {

                    a =

                        1 -

                        Math.min(

                            (p - 0.2) /
                            0.8,

                            1
                        );
                }


                disc.a =
                    a;
            }
        );
    }


    /*
     * Move particles.
     */
    moveDots(time) {

        this.dots.forEach(
            (dot) => {

                const v =

                    this.tweenValue(

                        0,

                        0.001,

                        1 -
                        dot.d.sx *
                        dot.d.sy,

                        "inExpo"
                    );


                /*
                 * Organic velocity variation.
                 */
                const breathingSpeed =

                    1 +

                    Math.sin(

                        time *
                        0.0008 +
                        dot.phase

                    ) *

                    0.08;


                dot.p =

                    (
                        dot.p +
                        v *
                        breathingSpeed
                    ) % 1;
            }
        );
    }


    /*
     * Main animation loop.
     */
    tick(time) {

        const {
            ctx
        } = this;


        /*
         * Clear canvas.
         */
        ctx.clearRect(

            0,
            0,

            this.canvas.width,
            this.canvas.height
        );


        ctx.save();


        /*
         * High-DPI support.
         */
        ctx.scale(

            this.render.dpi,
            this.render.dpi
        );


        /*
         * Draw the atmospheric glow first,
         * so everything appears inside it.
         */
        this.drawGlow(
            time
        );


        /*
         * Update.
         */
        this.moveDiscs(
            time
        );

        this.moveDots(
            time
        );


        /*
         * Render.
         */
        this.drawDiscs(
            time
        );

        this.drawDots(
            time
        );


        ctx.restore();


        /*
         * Continue animation.
         */
        requestAnimationFrame(
            this.tick.bind(this)
        );
    }
}


/*
 * Register custom element.
 */
customElements.define(
    "black-hole",
    BlackHole
);