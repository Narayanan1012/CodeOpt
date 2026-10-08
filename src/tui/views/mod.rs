pub mod analytics;
pub mod benchmarks;
pub mod home;
pub mod optimizing;
pub mod playground;
pub mod program_modal;

pub use analytics::render_analytics;
pub use benchmarks::render_benchmarks;
pub use home::render_home;
pub use optimizing::render_optimizing;
pub use playground::render_playground;
pub use program_modal::render_program_modal;
