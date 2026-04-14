package runtime

type PromptContextResidency string

const (
	PromptContextAlwaysLoaded  PromptContextResidency = "always_loaded"
	PromptContextTriggerLoaded PromptContextResidency = "trigger_loaded"
	PromptContextOnDemand      PromptContextResidency = "on_demand"
)

type PromptContextLayer struct {
	Name      string
	Residency PromptContextResidency
	Content   string
}

type PromptBudgetLayer struct {
	Name      string
	Residency PromptContextResidency
	Tokens    int
}
