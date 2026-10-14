// cargo test --release --test perf_bench -- --ignored --nocapture --test-threads=1
use eminus::model::{self, Clock};
use eminus::store;
use eminus::text::{self, Filter};
use serde_json::json;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

struct MeasuredAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

fn allocated(bytes: usize) {
    ALLOCS.fetch_add(1, Relaxed);
    let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(live, Relaxed);
}

unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() { allocated(layout.size()); }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let ptr = System.realloc(ptr, layout, size);
        if !ptr.is_null() {
            if size >= layout.size() { allocated(size - layout.size()); }
            else { LIVE.fetch_sub(layout.size() - size, Relaxed); ALLOCS.fetch_add(1, Relaxed); }
        }
        ptr
    }
}

#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

fn measure(name: &str, run: impl FnOnce()) {
    let base = LIVE.load(Relaxed);
    PEAK.store(base, Relaxed);
    ALLOCS.store(0, Relaxed);
    let started = Instant::now();
    run();
    let elapsed = started.elapsed();
    let peak = PEAK.load(Relaxed).saturating_sub(base);
    let allocations = ALLOCS.load(Relaxed);
    eprintln!("BENCH {name}: {elapsed:?}, {allocations} asignaciones, pico adicional {peak} bytes");
}

fn fixture(n_courses: usize, per_course: usize) -> (Vec<model::Course>, model::Acts) {
    let now = text::now();
    let rows: Vec<_> = (0..n_courses)
        .map(|i| json!({ "curso": { "idCurso": 100 + i, "nombre": format!("Materia de Redes y Programación {i} (12345)"), "fechaTerminoEpoch": now + 9_000_000 } }))
        .collect();
    let courses = model::courses_from(&rows, now);
    let mut acts = model::Acts::new();
    for c in &courses {
        let arows: Vec<_> = (0..per_course)
            .map(|j| json!({ "idActividad": j + 1, "titulo": format!("Tarea {j} de práctica de protocolos y programación"), "fechaTerminoEpoch": now + (j as i64) * 3600, "fechaTermino": "a" }))
            .collect();
        acts.insert(c.id.clone(), model::activities_from(&arows, c));
    }
    (courses, acts)
}

#[test]
#[ignore = "medición manual con datos sintéticos, ejecutar en release y con un solo hilo"]
fn bench_rebuild() {
    let (courses, acts) = fixture(30, 60);
    let clock = Clock { now: text::now(), today: text::start_of_today() };
    let filter = Filter::new("tarea redes");
    let empty = Filter::default();
    for (name, f) in [("con filtro", &filter), ("sin filtro", &empty)] {
        measure(name, || {
            let mut total = 0usize;
            for _ in 0..100 {
                total += model::agenda(&courses, &acts, f, false, &clock).len();
                total += model::miyu(&courses, &acts, f, &clock).len();
                let (rows, _) = model::grades(&courses, &acts, f);
                total += rows.len();
                total += model::content(&courses, &model::Content::new(), f).len();
                let c = model::counts(&courses, &acts, &clock);
                total += c.today;
            }
            std::hint::black_box(total);
        });
        measure(&format!("reutilizando filas {name}"), || {
            let (mut agenda, mut miyu, mut grades, mut content) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            let contents = model::Content::new();
            for _ in 0..100 {
                model::agenda_into(&mut agenda, &courses, &acts, f, false, &clock);
                model::miyu_into(&mut miyu, &courses, &acts, f, &clock);
                model::grades_into(&mut grades, &courses, &acts, f);
                model::content_into(&mut content, &courses, &contents, f);
                std::hint::black_box(model::counts(&courses, &acts, &clock));
            }
            std::hint::black_box((&agenda, &miyu, &grades, &content));
        });
    }
}

#[test]
#[ignore = "medición manual de caché, ejecutar en release y con un solo hilo"]
fn bench_cache() {
    let (courses, mut acts) = fixture(30, 60);
    let extra = "x".repeat(8192);
    for act in acts.values_mut().flatten() {
        let act = std::sync::Arc::get_mut(act).unwrap();
        act.title = format!("{} {extra}", act.title).into();
    }
    let dir = std::env::temp_dir().join(format!("eminus-perf-cache-{}", std::process::id()));
    let old = std::env::var_os("XDG_CACHE_HOME");
    std::env::set_var("XDG_CACHE_HOME", &dir);
    let now = text::now();
    measure("guardar caché", || store::save_courses("fixture", now, &courses, &acts));
    let size = std::fs::metadata(dir.join("eminus/cursos.json")).unwrap().len();
    assert!(size > 14_000_000);
    measure("leer caché", || {
        let (_, loaded_courses, loaded_acts) = store::load_courses("fixture", now).unwrap();
        assert_eq!(loaded_courses.len(), courses.len());
        assert_eq!(loaded_acts["100"][0].title, acts["100"][0].title);
        std::hint::black_box((&loaded_courses, &loaded_acts));
    });
    eprintln!("BENCH tamaño de caché: {size} bytes");
    match old {
        Some(value) => std::env::set_var("XDG_CACHE_HOME", value),
        None => std::env::remove_var("XDG_CACHE_HOME"),
    }
    std::fs::remove_dir_all(dir).unwrap();
}
