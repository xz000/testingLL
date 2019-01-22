using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillR2b : MonoBehaviour
{
    public CooldownImage MyImageScript;
    private float currentcooldown;
    public float cooldowntime = 5;
    public bool skillavaliable;
    public float LDspeed = 15;
    bool ImLSDS = false;
    MoveScript MS;

    // Use this for initialization
    void Start ()
    {
        currentcooldown = cooldowntime;
        MS = GetComponent<MoveScript>();
    }

    // Update is called once per frame
    void GoSkillR2b()
    {
        if (skillavaliable)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
        }
    }

    public void IdoDSWL()
    {
        if (ImLSDS)
        {
            ImLSDS = false;
            gameObject.GetComponent<StealthScript>().StealthEnd();
        }
    }

    public void Skill(Fix64Vector2 actionplacef)
    {
        Vector2 actionplace = actionplacef.ToV2();
        Vector2 singplace = transform.position;
        Vector2 skilldirection = actionplace - singplace;
        GetComponent<DoSkill>().BeforeSkill();
        MS.controllable = true;
        currentcooldown = 0;
        skillavaliable = false;
        ImLSDS = true;
        gameObject.GetComponent<RBScript>().GetPushed((Fix64Vector2)(skilldirection.normalized * LDspeed), Mathf.Infinity);
        gameObject.GetComponent<StealthScript>().StealthByTime(Mathf.Infinity, true);
    }

    void SkillR2bSetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
