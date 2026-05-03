import { describe, it, expect } from 'vitest';
import {
  userQuestionReducer,
  type PendingQuestion,
  type UserQuestionState,
} from '../hooks/useUserQuestion';

const Q1: PendingQuestion = {
  request_id: 'main-1',
  agent_id: 'main',
  question: 'What now?',
  options: ['proceed', 'abort'],
};

const Q2: PendingQuestion = {
  request_id: 'main-2',
  agent_id: 'main',
  question: 'Pick a path.',
  options: [],
};

const empty: UserQuestionState = { active: null };

describe('userQuestionReducer', () => {
  it('sets active on pending', () => {
    const next = userQuestionReducer(empty, { type: 'pending', payload: Q1 });
    expect(next.active).toEqual(Q1);
  });

  it('latest pending replaces previous in-flight question', () => {
    const after = userQuestionReducer(
      { active: Q1 },
      { type: 'pending', payload: Q2 },
    );
    expect(after.active).toEqual(Q2);
  });

  it('clears active on matching cancellation', () => {
    const after = userQuestionReducer(
      { active: Q1 },
      { type: 'cancelled', request_id: 'main-1' },
    );
    expect(after.active).toBeNull();
  });

  it('ignores cancellation for non-matching id', () => {
    // A cancellation event for a since-replaced question must not blank
    // a fresh active prompt.
    const after = userQuestionReducer(
      { active: Q2 },
      { type: 'cancelled', request_id: 'main-1' },
    );
    expect(after.active).toEqual(Q2);
  });

  it('clears active on matching submission', () => {
    const after = userQuestionReducer(
      { active: Q1 },
      { type: 'submitted', request_id: 'main-1' },
    );
    expect(after.active).toBeNull();
  });

  it('ignores submission for non-matching id', () => {
    const after = userQuestionReducer(
      { active: Q2 },
      { type: 'submitted', request_id: 'main-1' },
    );
    expect(after.active).toEqual(Q2);
  });

  it('cancellation while empty is a no-op', () => {
    const after = userQuestionReducer(
      empty,
      { type: 'cancelled', request_id: 'main-1' },
    );
    expect(after).toEqual(empty);
  });

  it('submission while empty is a no-op', () => {
    const after = userQuestionReducer(
      empty,
      { type: 'submitted', request_id: 'main-1' },
    );
    expect(after).toEqual(empty);
  });
});
