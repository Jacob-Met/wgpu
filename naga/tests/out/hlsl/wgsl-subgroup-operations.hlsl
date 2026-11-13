struct Structure {
    uint num_subgroups;
    uint subgroup_size;
};

struct ComputeInput_main {
    uint local_invocation_index : SV_GroupIndex;
};

[numthreads(1, 1, 1)]
void main(ComputeInput_main computeinput_main)
{
    Structure sizes = { (1u + WaveGetLaneCount() - 1u) / WaveGetLaneCount(), WaveGetLaneCount() };
    uint subgroup_id = computeinput_main.local_invocation_index / WaveGetLaneCount();
    uint subgroup_invocation_id = WaveGetLaneIndex();
    const uint4 _e7 = WaveActiveBallot(((subgroup_invocation_id & 1u) == 1u));
    const uint4 _e8 = WaveActiveBallot(true);
    const bool _e11 = WaveActiveAllTrue((subgroup_invocation_id != 0u));
    const bool _e14 = WaveActiveAnyTrue((subgroup_invocation_id == 0u));
    const uint _e15 = WaveActiveSum(subgroup_invocation_id);
    const uint _e16 = WaveActiveProduct(subgroup_invocation_id);
    const uint _e17 = WaveActiveMin(subgroup_invocation_id);
    const uint _e18 = WaveActiveMax(subgroup_invocation_id);
    const uint _e19 = WaveActiveBitAnd(subgroup_invocation_id);
    const uint _e20 = WaveActiveBitOr(subgroup_invocation_id);
    const uint _e21 = WaveActiveBitXor(subgroup_invocation_id);
    const uint _e22 = WavePrefixSum(subgroup_invocation_id);
    const uint _e23 = WavePrefixProduct(subgroup_invocation_id);
    const uint _e24 = subgroup_invocation_id + WavePrefixSum(subgroup_invocation_id);
    const uint _e25 = subgroup_invocation_id * WavePrefixProduct(subgroup_invocation_id);
    const uint _e26 = WaveReadLaneFirst(subgroup_invocation_id);
    const uint _e28 = WaveReadLaneAt(subgroup_invocation_id, 4u);
    const uint _e30 = WaveReadLaneAt(subgroup_invocation_id, 0u);
    const uint _e35 = WaveReadLaneAt(subgroup_invocation_id, ((sizes.subgroup_size - 1u) - subgroup_invocation_id));
    const uint _e38 = WaveReadLaneAt(subgroup_invocation_id, uint(int(subgroup_invocation_id)));
    const uint _e40 = WaveReadLaneAt(subgroup_invocation_id, WaveGetLaneIndex() + 1u);
    const uint _e42 = WaveReadLaneAt(subgroup_invocation_id, WaveGetLaneIndex() - 1u);
    const uint _e46 = WaveReadLaneAt(subgroup_invocation_id, WaveGetLaneIndex() ^ (sizes.subgroup_size - 1u));
    const uint _e48 = QuadReadLaneAt(subgroup_invocation_id, 4u);
    const uint _e50 = QuadReadLaneAt(subgroup_invocation_id, 0u);
    const uint _e51 = QuadReadAcrossX(subgroup_invocation_id);
    const uint _e52 = QuadReadAcrossY(subgroup_invocation_id);
    const uint _e53 = QuadReadAcrossDiagonal(subgroup_invocation_id);
    return;
}
