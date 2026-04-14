package mesh

import (
	"context"
	"errors"
	"log/slog"
	"sort"
)

type Judge interface {
	Score(ctx context.Context, taskClass string, candidates []CandidateReply) (map[string]int, error)
}

type Evaluator struct {
	judge  Judge
	logger *slog.Logger
}

func NewEvaluator(judge Judge) *Evaluator {
	return &Evaluator{
		judge:  judge,
		logger: slog.Default(),
	}
}

func (e *Evaluator) Evaluate(ctx context.Context, taskClass string, candidates []CandidateReply) (CandidateReply, []ScoreRecord, error) {
	if len(candidates) == 0 {
		return CandidateReply{}, nil, errors.New("mesh evaluator: no candidates")
	}

	scored := append([]CandidateReply(nil), candidates...)
	if e.judge != nil {
		judgeScores, err := e.judge.Score(ctx, taskClass, scored)
		if err == nil {
			for i := range scored {
				scored[i].JudgeScore = judgeScores[scored[i].AgentID]
			}
		}
	}

	sort.Slice(scored, func(i, j int) bool {
		left, right := scored[i], scored[j]
		leftFinal := left.Stage == "final"
		rightFinal := right.Stage == "final"
		if leftFinal != rightFinal {
			return leftFinal
		}
		if left.PassedChecks != right.PassedChecks {
			return left.PassedChecks
		}
		if left.DeterministicScore != right.DeterministicScore {
			return left.DeterministicScore > right.DeterministicScore
		}
		if left.JudgeScore != right.JudgeScore {
			return left.JudgeScore > right.JudgeScore
		}
		if left.Latency != right.Latency {
			return left.Latency < right.Latency
		}
		return left.AgentID < right.AgentID
	})

	winner := scored[0]
	updates := make([]ScoreRecord, 0, len(scored))
	for _, candidate := range scored {
		update := ScoreRecord{
			AgentID:      candidate.AgentID,
			TaskClass:    taskClass,
			TasksSeen:    1,
			AvgLatencyMS: candidate.Latency.Milliseconds(),
		}
		if candidate.Stage == "final" && candidate.PassedChecks {
			update.SuccessCount = 1
		} else {
			update.FailureCount = 1
		}
		if candidate.AgentID == winner.AgentID {
			update.TasksWon = 1
		}
		updates = append(updates, update)
	}

	e.logger.Debug("mesh evaluator winner",
		"task_class", taskClass,
		"winner_agent", winner.AgentID,
		"candidate_count", len(scored),
		"winner_stage", winner.Stage,
	)
	return winner, updates, nil
}
