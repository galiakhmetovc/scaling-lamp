package runtime

type ProviderRequestContract struct {
	Transport    TransportPolicy
	RequestShape RequestShapePolicy
	Prompt       PromptPolicy
	Tools        ToolPolicy
}

type MemoryContract struct {
	Offload       OffloadPolicy
	Summarization SummarizationPolicy
	Workspace     WorkspacePolicy
}

type ExecutionContract struct {
	Tools ToolPolicy
}

type DisplayContract struct {
	Display DisplayPolicy
	Prompt  PromptPolicy
}

type ResolvedContextContracts struct {
	ProviderRequest ProviderRequestContract
	Memory          MemoryContract
	Execution       ExecutionContract
	Display         DisplayContract
}

func ResolveContextContracts(policy EffectiveContextPolicy) ResolvedContextContracts {
	return ResolvedContextContracts{
		ProviderRequest: ProviderRequestContract{
			Transport:    policy.Transport,
			RequestShape: policy.RequestShape,
			Prompt:       policy.Prompt,
			Tools:        policy.Tools,
		},
		Memory: MemoryContract{
			Offload:       policy.Offload,
			Summarization: policy.Summarization,
			Workspace:     policy.Workspace,
		},
		Execution: ExecutionContract{
			Tools: policy.Tools,
		},
		Display: DisplayContract{
			Display: policy.Display,
			Prompt:  policy.Prompt,
		},
	}
}
