using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillC : UnityEngine.MonoBehaviour
{
    public Toggle C1;
    public Toggle C2;
    public Toggle C3;
    public Toggle C4;
    public Image IconC;
    public SkillsLink sls;

    public void SetC()
    {
        AllCOff();
        if (C1.isOn)
        {
            sls.mySoldier.GetComponent<SkillC1>().MyImageScript = IconC.GetComponent<CooldownImage>();
            sls.KeyCSkill = SkillCode.SkillC1;
            return;
        }
        if (C2.isOn)
        {
            sls.mySoldier.GetComponent<SkillC2>().MyImageScript = IconC.GetComponent<CooldownImage>();
            sls.KeyCSkill = SkillCode.SkillC2;
            return;
        }
        if (C3.isOn)
        {
            IconC.GetComponent<CooldownImage>().Fif = sls.mySoldier.GetComponent<SkillC3>().CalcFA;
            sls.KeyCSkill = SkillCode.SkillC3;
            return;
        }
        if (C4.isOn)
        {
            sls.mySoldier.GetComponent<SkillC4>().MyImageScript = IconC.GetComponent<CooldownImage>();
            sls.KeyCSkill = SkillCode.SkillC4;
            return;
        }
        IconC.GetComponent<CooldownImage>().enabled = true;
    }

    public void AllCOff()
    {
        IconC.GetComponent<CooldownImage>().IconFillAmount = 1;
    }
}
