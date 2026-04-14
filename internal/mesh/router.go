package mesh

import (
	"context"
	"log/slog"
	"math/rand"
	"sort"
)

type RouterConfig struct {
	AgentID         string
	ColdStartFanout int
	ExplorationRate float64
}

type Router struct {
	registry Registry
	config   RouterConfig
	random   *rand.Rand
	logger   *slog.Logger
}

func NewRouter(registry Registry, config RouterConfig) *Router {
	return &Router{
		registry: registry,
		config:   config,
		random:   rand.New(rand.NewSource(1)),
		logger:   slog.Default(),
	}
}

func (r *Router) SelectPeers(ctx context.Context, ownerAgentID, taskClass string) ([]PeerDescriptor, error) {
	online, err := r.registry.ListOnline(ctx)
	if err != nil {
		return nil, err
	}

	candidates := make([]PeerDescriptor, 0, len(online))
	for _, peer := range online {
		if peer.AgentID == ownerAgentID || peer.Status != "idle" {
			continue
		}
		candidates = append(candidates, peer)
	}

	scores, err := r.registry.ListScores(ctx, taskClass)
	if err != nil {
		return nil, err
	}
	scoreByAgent := make(map[string]ScoreRecord, len(scores))
	for _, score := range scores {
		scoreByAgent[score.AgentID] = score
	}

	sort.Slice(candidates, func(i, j int) bool {
		left, leftOK := scoreByAgent[candidates[i].AgentID]
		right, rightOK := scoreByAgent[candidates[j].AgentID]
		if !leftOK && !rightOK {
			return candidates[i].AgentID < candidates[j].AgentID
		}
		if !leftOK {
			return false
		}
		if !rightOK {
			return true
		}
		if left.TasksWon != right.TasksWon {
			return left.TasksWon > right.TasksWon
		}
		if left.SuccessCount != right.SuccessCount {
			return left.SuccessCount > right.SuccessCount
		}
		if left.AvgLatencyMS != right.AvgLatencyMS {
			return left.AvgLatencyMS < right.AvgLatencyMS
		}
		return candidates[i].AgentID < candidates[j].AgentID
	})

	fanout := r.config.ColdStartFanout
	if fanout <= 0 {
		fanout = 1
	}
	if len(candidates) > fanout {
		candidates = candidates[:fanout]
	}

	r.logger.Debug("mesh router selection",
		"owner_agent", ownerAgentID,
		"task_class", taskClass,
		"selected_count", len(candidates),
		"scored_peers", len(scoreByAgent),
	)
	return candidates, nil
}
