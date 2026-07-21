document.documentElement.classList.add("js");

const revealItems = document.querySelectorAll("[data-reveal]");
const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

if (reduceMotion.matches || !("IntersectionObserver" in window)) {
  revealItems.forEach((item) => item.classList.add("is-visible"));
} else {
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        entry.target.classList.add("is-visible");
        observer.unobserve(entry.target);
      }
    },
    { rootMargin: "0px 0px -8%", threshold: 0.12 },
  );

  revealItems.forEach((item) => observer.observe(item));
}

const header = document.querySelector(".site-header");
const updateHeader = () => {
  header?.classList.toggle("is-scrolled", window.scrollY > 18);
};

updateHeader();
window.addEventListener("scroll", updateHeader, { passive: true });
