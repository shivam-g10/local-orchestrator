pub mod ai_news_digest;
pub mod personal_reports;
pub mod trial_activation_nudge;

pub use ai_news_digest::{
    AiNewsDigestWorkflowConfig, ensure_dummy_data as ensure_ai_news_digest_dummy_data,
    run_ai_news_digest_workflow,
};
pub use personal_reports::{ensure_dummy_data, run_personal_reports_workflow};
pub use trial_activation_nudge::{
    TrialActivationNudgeWorkflowConfig,
    ensure_dummy_data as ensure_trial_activation_nudge_dummy_data,
    run_trial_activation_nudge_workflow,
};
