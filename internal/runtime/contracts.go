package runtime

type ResolvedContracts struct {
	ProviderRequest ProviderRequestContract
	Memory          MemoryContract
}

type ProviderRequestContract struct {
	Transport TransportContract
}

type TransportContract struct {
	ID       string
	Endpoint EndpointPolicy
}

type EndpointPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   EndpointParams
}

type EndpointParams struct {
	BaseURL      string            `yaml:"base_url"`
	Path         string            `yaml:"path"`
	ExtraHeaders map[string]string `yaml:"extra_headers"`
}

type MemoryContract struct {
	ID      string
	Offload OffloadPolicy
}

type OffloadPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   OffloadParams
}

type OffloadParams struct {
	MaxChars int `yaml:"max_chars"`
}
