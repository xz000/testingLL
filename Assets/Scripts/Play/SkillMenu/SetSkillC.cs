using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class SetSkillC : MonoBehaviour
{
    public Toggle C1;
    public Toggle C2;
    public Toggle C3;
    public Toggle C4;
    public Image IconC;
    public SkillsLink sls;

    public void SetC()
    {
        sls.KeyCSkill = null;
        if (C1.isOn)
        {
            sls.KeyCSkill = SkillCode.SkillC1;
            return;
        }
        if (C2.isOn)
        {
            sls.KeyCSkill = SkillCode.SkillC2;
            return;
        }
        if (C3.isOn)
        {
            sls.KeyCSkill = SkillCode.SkillC3;
            return;
        }
        if (C4.isOn)
        {
            sls.KeyCSkill = SkillCode.SkillC4;
            return;
        }
    }
}
