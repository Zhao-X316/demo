//! M2 业务服务。

pub mod answer_sheet;
pub mod answer_sheet_page;
pub mod answer_source;
pub mod assessment;
pub mod dictation_pipeline;
pub mod fixed_paper;
pub mod grading;
pub mod objective;
pub mod ordered_activation;
pub mod ordered_intake;
pub mod ordered_retake;
pub mod ordinary_structure;
pub mod page_cycle;
pub mod papers;
pub mod question_ingest;
pub mod subjective;

#[cfg(test)]
mod ordinary_structure_tests;

#[cfg(test)]
mod answer_sheet_page_tests;

#[cfg(test)]
mod dictation_pipeline_tests;

#[cfg(test)]
mod subjective_tests;
