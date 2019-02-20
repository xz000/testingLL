using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillF : MonoBehaviour
{
    public Toggle F1;
    public Image IconF;
    GameObject Soldier;

    public void SetF()
    {
        AllFOff();
        IconF.GetComponent<CooldownImage>().enabled = true;
        if (F1.isOn)
        {
            if (GetComponent<SkillsLink>().mySoldier != null)
                IconF.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<SelfExplodeScript>().CalcFA;
            GetComponent<SkillsLink>().KeyFSkill = SkillCode.TestSkill03;
            return;
        }
    }

    void AllFOff()
    {
        IconF.GetComponent<CooldownImage>().enabled = false;
        GetComponent<SkillsLink>().KeyFSkill = null;
    }
}
