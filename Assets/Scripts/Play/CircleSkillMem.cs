using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class CircleSkillMem : MonoBehaviour
{
    public int[] csm;
    public void SetCircleSL(int[] sld)
    {
        csm = sld;
        int max = (int)SkillCode.FireStop;
        for (int i = 0; i < max; i++)
        {
            SkillCode sc = (SkillCode)i;
            SendMessage(sc.ToString() + "SetLevel", sld[i]);
        }
    }
}
