package skills

import "sort"

type Bundle struct {
	Name        string
	Description string
	Version     string
	License     string
	Path        string
	Prompt      string
	AllowedTools []string
	Scripts      []string
	References   []string
	Assets       []string
}

type Loader interface {
	Load(role string) ([]Bundle, error)
}

type Catalog interface {
	List() ([]Bundle, error)
	Get(name string) (Bundle, bool, error)
}

type StaticLoader struct {
	Bundles []Bundle
}

func (l StaticLoader) Load(string) ([]Bundle, error) {
	return l.Bundles, nil
}

func Summaries(bundles []Bundle) []Summary {
	items := make([]Summary, 0, len(bundles))
	for _, bundle := range bundles {
		items = append(items, Summary{
			Name:        bundle.Name,
			Description: bundle.Description,
		})
	}
	sort.Slice(items, func(i, j int) bool {
		return items[i].Name < items[j].Name
	})
	return items
}
