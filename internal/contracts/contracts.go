package contracts

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
	Auth     AuthPolicy
	Retry    RetryPolicy
	Timeout  TimeoutPolicy
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
	Method       string            `yaml:"method"`
	ExtraHeaders map[string]string `yaml:"extra_headers"`
}

type AuthPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   AuthParams
}

type AuthParams struct {
	Header      string `yaml:"header"`
	Prefix      string `yaml:"prefix"`
	ValueEnvVar string `yaml:"value_env_var"`
}

type RetryPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   RetryParams
}

type RetryParams struct {
	MaxAttempts     int      `yaml:"max_attempts"`
	BaseDelay       string   `yaml:"base_delay"`
	MaxDelay        string   `yaml:"max_delay"`
	RetryOnStatuses []int    `yaml:"retry_on_statuses"`
	RetryOnErrors   []string `yaml:"retry_on_errors"`
}

type TimeoutPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   TimeoutParams
}

type TimeoutParams struct {
	Total   string `yaml:"total"`
	Connect string `yaml:"connect"`
	Idle    string `yaml:"idle"`
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
