package memory

import (
	"sort"
	"strings"
	"time"
)

type InMemorySemanticStore struct {
	data map[string]Document
}

func NewInMemorySemanticStore() *InMemorySemanticStore {
	return &InMemorySemanticStore{data: map[string]Document{}}
}

func (s *InMemorySemanticStore) UpsertDocument(doc Document) error {
	if strings.TrimSpace(doc.DocKey) == "" {
		doc.DocKey = strings.TrimSpace(doc.Kind + ":" + doc.SessionID + ":" + doc.Title)
	}
	if doc.UpdatedAt.IsZero() {
		doc.UpdatedAt = time.Now().UTC()
	}
	s.data[doc.DocKey] = doc
	return nil
}

func (s *InMemorySemanticStore) Search(q RecallQuery) ([]RecallItem, error) {
	needle := strings.ToLower(strings.TrimSpace(q.Text))
	kinds := NormalizeRecallKinds(q.Kinds)
	allowedKinds := map[string]struct{}{}
	for _, kind := range kinds {
		allowedKinds[kind] = struct{}{}
	}
	limit := q.Limit
	if limit <= 0 {
		limit = 3
	}
	type scored struct {
		item RecallItem
	}
	var out []scored
	for _, doc := range s.data {
		if len(allowedKinds) > 0 {
			if _, ok := allowedKinds[strings.ToLower(strings.TrimSpace(doc.Kind))]; !ok {
				continue
			}
		}
		if doc.Scope != ScopeGlobal {
			if doc.ChatID != q.ChatID {
				continue
			}
			if doc.SessionID != "" && q.SessionID != "" && doc.SessionID != q.SessionID {
				continue
			}
		}
		hay := strings.ToLower(doc.Title + "\n" + doc.Body)
		score := 0.0
		if needle == "" {
			score = 1
		} else {
			for _, token := range strings.Fields(needle) {
				if strings.Contains(hay, token) {
					score += 1
				}
			}
		}
		if score <= 0 {
			continue
		}
		out = append(out, scored{item: RecallItem{
			DocKey: doc.DocKey,
			Kind:  doc.Kind,
			Title: doc.Title,
			Body:  doc.Body,
			Score: score,
		}})
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].item.Score == out[j].item.Score {
			return out[i].item.Title < out[j].item.Title
		}
		return out[i].item.Score > out[j].item.Score
	})
	items := make([]RecallItem, 0, min(limit, len(out)))
	for i := 0; i < len(out) && i < limit; i++ {
		items = append(items, out[i].item)
	}
	return items, nil
}

func (s *InMemorySemanticStore) Get(docKey string) (Document, bool, error) {
	doc, ok := s.data[strings.TrimSpace(docKey)]
	if !ok {
		return Document{}, false, nil
	}
	return doc, true, nil
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
