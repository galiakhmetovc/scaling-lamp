package mcp

func Names(servers []Server) []string {
	out := make([]string, 0, len(servers))
	for _, server := range servers {
		out = append(out, server.Name)
	}
	return out
}
